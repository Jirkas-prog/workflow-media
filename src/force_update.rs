//! External, signed update transaction. No termination of download processes.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
struct Recovery {
    exe: PathBuf,
    paused: HashSet<String>,
}

fn read_queue(data: &Path) -> Result<Value> {
    let path = data.join("queue.json");
    if !path.exists() {
        return Ok(serde_json::json!({"Jobs":[]}));
    }
    let state: Value = serde_json::from_slice(&fs::read(path)?)?;
    if !state.get("Jobs").is_some_and(Value::is_array) {
        bail!("Invalid queue / Neplatná fronta");
    }
    Ok(state)
}
fn paused_ids(state: &Value) -> HashSet<String> {
    state["Jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|j| j["State"] == "Paused")
        .filter_map(|j| j["Id"].as_str().map(str::to_owned))
        .collect()
}
fn resume_interrupted(state: &mut Value, paused: &HashSet<String>) {
    for job in state["Jobs"].as_array_mut().into_iter().flatten() {
        if job["State"] == "Paused"
            && job["Message"] == "@interrupted"
            && job["Id"].as_str().is_some_and(|id| !paused.contains(id))
        {
            job["State"] = "Queued".into();
            job["Message"] = "".into();
        }
    }
}
fn lock(path: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    fs2::FileExt::try_lock_exclusive(&file)
        .context("Application or updater is busy / Aplikace nebo aktualizátor právě pracuje")?;
    Ok(file)
}

#[cfg(windows)]
pub fn run(args: &[String]) -> Result<()> {
    let option = |key: &str| {
        args.windows(2)
            .find(|w| w[0] == key)
            .map(|w| PathBuf::from(&w[1]))
    };
    let data = option("--data").unwrap_or_else(crate::platform::app_data);
    if !data.is_absolute() {
        bail!("--data must be absolute");
    }
    fs::create_dir_all(&data)?;
    let _updater_lock = lock(&data.join("external-update.lock"))?;
    let instance = crate::maintenance::read_instance(&data)
        .filter(|i| native::process_path(i.pid).is_some_and(|p| native::same_path(&p, &i.exe)));
    let exe = if let Some(path) = option("--application") {
        path
    } else if let Some(i) = &instance {
        i.exe.clone()
    } else {
        native::discover()?
    };
    if !exe.is_absolute() || !exe.is_file() {
        bail!("Application not found / Aplikace nebyla nalezena");
    }
    let installed_version = native::version(&exe)?;
    let preset = option("--preset")
        .or_else(|| {
            instance
                .as_ref()
                .filter(|i| native::same_path(&i.exe, &exe))
                .map(|i| i.preset.clone())
        })
        .unwrap_or_else(|| exe.parent().unwrap().join("WorkFlow Media.preset.json"));
    if !preset.is_file() || !preset.is_absolute() {
        bail!("Existing preset not found / Nastavení existující aplikace nebylo nalezeno");
    }
    let pids = native::processes_at(&exe)?;
    if pids.len() > 1 {
        bail!("Multiple instances detected / Nalezeno více instancí");
    }
    let recovery_path = data.join("external-update-recovery.json");
    let recovery = if recovery_path.exists() {
        let old: Recovery = serde_json::from_slice(&fs::read(&recovery_path)?)?;
        if !native::same_path(&old.exe, &exe) {
            bail!(
                "An unfinished update belongs to another application / Nedokončená aktualizace patří jiné aplikaci"
            );
        }
        old
    } else {
        Recovery {
            exe: exe.clone(),
            paused: paused_ids(&read_queue(&data)?),
        }
    };
    crate::model::save_json(&recovery_path, &recovery)?;
    if let Some(&pid) = pids.first() {
        // The new protocol is tied to this process lifetime; stale requests cannot close a restart.
        if let Some(i) = instance.as_ref().filter(|i| i.pid == pid) {
            crate::model::save_json(&data.join("update-stop.json"), i)?;
            native::wake(pid);
        } else {
            if installed_version < semver::Version::new(0, 8, 0)
                || installed_version > semver::Version::new(0, 8, 5)
            {
                bail!(
                    "This version needs the update command in Settings / Tato verze vyžaduje aktualizaci v Nastavení"
                );
            }
            native::legacy_quit(pid)?;
        }
        native::wait_exit(pid)?;
    }
    // Acquiring this lock proves that the UI, scheduler and workers have finished saving.
    let instance_lock = lock(&data.join("instance.lock"))?;
    let cache = tempfile::Builder::new()
        .prefix("external-update-")
        .tempdir_in(&data)?;
    let backup = exe.with_extension("previous");
    let mut replaced = false;
    let update = (|| -> Result<()> {
        let manifest = crate::updater::latest()?;
        let remote = semver::Version::parse(&manifest.version)?;
        if remote < installed_version {
            return Ok(());
        } // Never downgrade a newer installation.
        if remote == installed_version {
            use sha2::{Digest, Sha256};
            if hex::encode(Sha256::digest(fs::read(&exe)?)) == manifest.sha256 {
                return Ok(());
            }
        }
        let download = crate::updater::download(&manifest, cache.path())?;
        // Stage on the destination volume before replacing the running executable.
        let staged = exe.with_extension("update-new");
        fs::copy(&download, &staged)?;
        fs::copy(&exe, &backup)?;
        if let Err(error) = crate::platform::replace_file(&staged, &exe) {
            let _ = fs::remove_file(&staged);
            return Err(error);
        }
        replaced = true;
        Ok(())
    })();
    // Also restore the previous app after a network/signature/disk failure.
    let mut state = read_queue(&data)?;
    resume_interrupted(&mut state, &recovery.paused);
    if data.join("queue.json").exists() {
        crate::model::save_json(&data.join("queue.json"), &state)?;
    }
    drop(instance_lock);
    let launch = || {
        crate::platform::command(&exe)
            .arg("--data")
            .arg(&data)
            .arg("--preset")
            .arg(&preset)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    };
    if let Err(error) = launch() {
        if replaced {
            crate::platform::replace_file(&backup, &exe)
                .context("Cannot restore previous version / Nelze obnovit předchozí verzi")?;
            launch().context("Cannot restart previous version / Nelze spustit předchozí verzi")?;
        }
        return Err(error).context("Cannot restart application / Nelze znovu spustit aplikaci");
    }
    let _ = fs::remove_file(&recovery_path);
    let _ = fs::remove_file(data.join("update-stop.json"));
    update.context("Update failed; previous application restarted / Aktualizace selhala; původní aplikace byla znovu spuštěna")
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::{Diagnostics::ToolHelp::*, Threading::*},
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };
    pub fn same_path(a: &Path, b: &Path) -> bool {
        match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a
                .to_string_lossy()
                .eq_ignore_ascii_case(&b.to_string_lossy()),
            _ => false,
        }
    }
    pub fn process_path(pid: u32) -> Option<PathBuf> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 32768];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
            CloseHandle(handle);
            (ok != 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..size as usize])))
        }
    }
    pub fn processes_at(exe: &Path) -> Result<Vec<u32>> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of_val(&entry) as u32;
            let mut ok = Process32FirstW(snapshot, &mut entry);
            let mut found = vec![];
            while ok != 0 {
                if process_path(entry.th32ProcessID).is_some_and(|p| same_path(&p, exe)) {
                    found.push(entry.th32ProcessID);
                }
                ok = Process32NextW(snapshot, &mut entry);
            }
            CloseHandle(snapshot);
            Ok(found)
        }
    }
    fn windows(pid: Option<u32>, class: Option<&str>) -> Vec<HWND> {
        struct Search<'a> {
            pid: Option<u32>,
            class: Option<&'a str>,
            found: Vec<HWND>,
        }
        unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
            let search = unsafe { &mut *(data as *mut Search<'_>) };
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
            }
            let mut buf = [0u16; 128];
            let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
            if search.pid.is_none_or(|p| p == pid)
                && search
                    .class
                    .is_none_or(|c| String::from_utf16_lossy(&buf[..n.max(0) as usize]) == c)
            {
                search.found.push(hwnd);
            }
            1
        }
        let mut search = Search {
            pid,
            class,
            found: vec![],
        };
        unsafe {
            EnumWindows(Some(visit), &mut search as *mut _ as isize);
        }
        search.found
    }
    pub fn discover() -> Result<PathBuf> {
        let mut paths: Vec<PathBuf> = Vec::new();
        for hwnd in windows(None, None) {
            let mut title = [0u16; 256];
            let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
            let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
            if title.starts_with("WorkFlow Media · ") && !title.contains("Setup") {
                let mut pid = 0;
                unsafe {
                    GetWindowThreadProcessId(hwnd, &mut pid);
                }
                if let Some(path) = process_path(pid) {
                    if !paths.iter().any(|p| same_path(p, &path)) {
                        paths.push(path);
                    }
                }
            }
        }
        if paths.len() == 1 {
            return Ok(paths.remove(0));
        }
        if paths.len() > 1 {
            bail!(
                "Multiple WorkFlow windows found; use --application / Nalezeno více aplikací; použijte --application"
            );
        }
        let beside = std::env::current_exe()?
            .parent()
            .unwrap()
            .join("WorkFlow Media.exe");
        let documents = directories::UserDirs::new().and_then(|d| {
            d.document_dir()
                .map(|p| p.join("WorkFlow Media/WorkFlow Media.exe"))
        });
        std::iter::once(beside)
            .chain(documents)
            .find(|p| p.is_file())
            .context("Start WorkFlow Media first / Nejprve spusťte WorkFlow Media")
    }
    pub fn version(exe: &Path) -> Result<semver::Version> {
        let filename = crate::platform::wide(exe);
        unsafe {
            let len = GetFileVersionInfoSizeW(filename.as_ptr(), std::ptr::null_mut());
            if len == 0 {
                bail!("Missing application version / Chybí verze aplikace");
            }
            let mut bytes = vec![0u8; len as usize];
            if GetFileVersionInfoW(filename.as_ptr(), 0, len, bytes.as_mut_ptr().cast()) == 0 {
                bail!("Cannot read application version / Nelze načíst verzi aplikace");
            }
            let mut fixed = std::ptr::null_mut();
            let mut size = 0;
            if VerQueryValueW(
                bytes.as_ptr().cast(),
                crate::platform::wide("\\").as_ptr(),
                &mut fixed,
                &mut size,
            ) == 0
                || size < std::mem::size_of::<VS_FIXEDFILEINFO>() as u32
            {
                bail!("Invalid application version");
            }
            let info = std::ptr::read_unaligned(fixed as *const VS_FIXEDFILEINFO);
            // All Rust releases carry this product identity. Never replace an unrelated EXE.
            let mut product = std::ptr::null_mut();
            if VerQueryValueW(
                bytes.as_ptr().cast(),
                crate::platform::wide("\\StringFileInfo\\000004b0\\ProductName").as_ptr(),
                &mut product,
                &mut size,
            ) == 0
            {
                bail!("Not a WorkFlow Media executable");
            }
            let product = String::from_utf16_lossy(std::slice::from_raw_parts(
                product as *const u16,
                size.saturating_sub(1) as usize,
            ));
            if product != "WorkFlow Media" {
                bail!("Not a WorkFlow Media executable");
            }
            let mut description = std::ptr::null_mut();
            if VerQueryValueW(
                bytes.as_ptr().cast(),
                crate::platform::wide("\\StringFileInfo\\000004b0\\FileDescription").as_ptr(),
                &mut description,
                &mut size,
            ) == 0
                || String::from_utf16_lossy(std::slice::from_raw_parts(
                    description as *const u16,
                    size.saturating_sub(1) as usize,
                )) != "WorkFlow Media · Rust"
            {
                bail!(
                    "Select the main Rust application, not an installer / Vyberte hlavní Rust aplikaci, nikoli instalátor"
                );
            }
            Ok(semver::Version::new(
                (info.dwFileVersionMS >> 16) as u64,
                (info.dwFileVersionMS & 65535) as u64,
                (info.dwFileVersionLS >> 16) as u64,
            ))
        }
    }
    pub fn wake(pid: u32) {
        for hwnd in windows(Some(pid), None) {
            unsafe {
                PostMessageW(hwnd, WM_PAINT, 0, 0);
            }
        }
    }
    pub fn wait_exit(pid: u32) -> Result<()> {
        unsafe {
            let handle = OpenProcess(SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                if process_path(pid).is_none() {
                    return Ok(());
                }
                bail!("Cannot wait for application shutdown");
            }
            let result = WaitForSingleObject(handle, 120_000);
            CloseHandle(handle);
            if result != WAIT_OBJECT_0 {
                bail!(
                    "Application did not stop; no files replaced / Aplikace se neukončila; soubory nebyly nahrazeny"
                );
            }
        }
        Ok(())
    }
    pub fn legacy_quit(pid: u32) -> Result<()> {
        // Versions 0.8.0–0.8.5 predate the cooperative protocol. Invoke their existing
        // native tray Quit action, discovering its actual menu ID (which changes with language).
        // No process kill, injected code or guessed menu command IDs.
        let main = windows(Some(pid), None)
            .into_iter()
            .find(|&hwnd| {
                let mut title = [0u16; 256];
                let n = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
                String::from_utf16_lossy(&title[..n.max(0) as usize])
                    .starts_with("WorkFlow Media · ")
            })
            .context("Legacy application window not found")?;
        let press = |key| {
            unsafe {
                let scan = MapVirtualKeyW(key, MAPVK_VK_TO_VSC) as isize;
                PostMessageW(main, WM_KEYDOWN, key as usize, 1 | (scan << 16));
                PostMessageW(
                    main,
                    WM_KEYUP,
                    key as usize,
                    1 | (scan << 16) | (3isize << 30),
                );
                PostMessageW(main, WM_PAINT, 0, 0);
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        press(0x1b); // Clear previous focus/dialog before opening a fresh Quit modal.
        let tray = windows(Some(pid), Some("tray_icon_app"))
            .into_iter()
            .next()
            .context("Cannot find legacy tray / Nelze najít ikonu starší aplikace")?;
        unsafe {
            if PostMessageW(tray, 6002, 0, WM_RBUTTONDOWN as isize) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        let mut command = None;
        for _ in 0..50 {
            for hwnd in windows(Some(pid), Some("#32768")) {
                unsafe {
                    let mut info: MENUBARINFO = std::mem::zeroed();
                    info.cbSize = std::mem::size_of_val(&info) as u32;
                    if GetMenuBarInfo(hwnd, OBJID_CLIENT, 0, &mut info) == 0 {
                        continue;
                    }
                    for index in 0..GetMenuItemCount(info.hMenu) {
                        let mut label = [0u16; 256];
                        let n = GetMenuStringW(
                            info.hMenu,
                            index as u32,
                            label.as_mut_ptr(),
                            label.len() as i32,
                            MF_BYPOSITION,
                        );
                        let label =
                            String::from_utf16_lossy(&label[..n.max(0) as usize]).replace('&', "");
                        if matches!(label.as_str(), "Quit" | "Ukončit") {
                            let id = GetMenuItemID(info.hMenu, index);
                            if id != u32::MAX {
                                command = Some(id);
                            }
                        }
                    }
                }
            }
            if command.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        unsafe {
            SendMessageTimeoutW(
                tray,
                WM_CANCELMODE,
                0,
                0,
                SMTO_ABORTIFHUNG,
                2000,
                std::ptr::null_mut(),
            );
        }
        let command = command.context("Cannot invoke legacy Quit; no files replaced / Starší aplikaci nelze ukončit; soubory nebyly nahrazeny")?;
        unsafe {
            if SendMessageTimeoutW(
                tray,
                WM_COMMAND,
                command as usize,
                0,
                SMTO_ABORTIFHUNG,
                2000,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        wake(pid);
        // The fresh modal has no keyboard focus. Tab focuses Confirm and Enter
        // invokes the application's normal shutdown. Messages go
        // only to this process's window, never to the global input queue.
        std::thread::sleep(Duration::from_millis(500));
        unsafe {
            SetForegroundWindow(main);
        }
        press(0x09);
        press(0x0d);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_preserves_manual_pauses_completed_jobs_and_unknown_fields() {
        let mut queue = serde_json::json!({"FutureField":42,"Settings":{"QueuePaused":true},"Jobs":[
            {"Id":"active","State":"Downloading","Message":"","Unknown":"keep"},
            {"Id":"old","State":"Paused","Message":"@interrupted"},
            {"Id":"manual","State":"Paused","Message":"@paused"},
            {"Id":"queued","State":"Queued"},{"Id":"done","State":"Completed"}
        ]});
        let paused = paused_ids(&queue);
        queue["Jobs"][0]["State"] = "Paused".into();
        queue["Jobs"][0]["Message"] = "@interrupted".into();
        resume_interrupted(&mut queue, &paused);
        assert_eq!(queue["Jobs"][0]["State"], "Queued");
        assert_eq!(queue["Jobs"][0]["Unknown"], "keep");
        assert_eq!(queue["Jobs"][1]["State"], "Paused");
        assert_eq!(queue["Jobs"][2]["State"], "Paused");
        assert_eq!(queue["Jobs"][3]["State"], "Queued");
        assert_eq!(queue["Jobs"][4]["State"], "Completed");
        assert_eq!(queue["Settings"]["QueuePaused"], true);
        assert_eq!(queue["FutureField"], 42);
    }
}
