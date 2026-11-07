#[cfg(not(windows))]
use anyhow::Context;
use anyhow::{Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Keep the event loop responsive when Windows suppresses redraws for a hidden window.
#[derive(Clone)]
pub struct WindowWake {
    ctx: eframe::egui::Context,
    #[cfg(windows)]
    hwnd: usize,
}
impl WindowWake {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            ctx: cc.egui_ctx.clone(),
            #[cfg(windows)]
            hwnd: {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                match cc.window_handle().map(|h| h.as_raw()) {
                    Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get() as usize,
                    _ => 0,
                }
            },
        }
    }
    pub fn request(&self) {
        self.ctx.request_repaint();
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsIconic, IsWindowVisible, PostMessageW, WM_PAINT,
            };
            let hwnd = self.hwnd as windows_sys::Win32::Foundation::HWND;
            // winit normally uses RedrawWindow, which does not deliver paint events to
            // hidden windows. Post the event without showing or focusing the window.
            // The UI thread still owns all event handling and rendering.
            if !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 } {
                unsafe { PostMessageW(hwnd, WM_PAINT, 0, 0) };
            }
        }
    }
}

pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}
pub fn app_data() -> PathBuf {
    #[cfg(windows)]
    {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("WorkFlow Media Rust");
    }
    #[cfg(not(windows))]
    {
        directories::BaseDirs::new()
            .map(|d| d.data_local_dir().join("workflow-media"))
            .unwrap_or_else(|| PathBuf::from(".workflow-media"))
    }
}
pub fn legacy_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("WorkFlow Media/data"))
}
#[cfg(windows)]
fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
pub fn replace_file(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        if unsafe {
            MoveFileExW(
                wide(from).as_ptr(),
                wide(to).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(from, to)?;
        Ok(())
    }
}
pub fn open_location(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        if path.is_file() {
            command("explorer.exe")
                .arg(format!("/select,{}", path.display()))
                .spawn()?;
        } else {
            open::that(path)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        open::that(if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        })?;
        Ok(())
    }
}
pub fn copy_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("File not found / Soubor nenalezen");
    }
    #[cfg(windows)]
    {
        use clipboard_win::Setter;
        let _guard = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|e| anyhow::anyhow!("Clipboard: {e}"))?;
        clipboard_win::formats::FileList
            .write_clipboard(&[path.to_string_lossy().as_ref()])
            .map_err(|e| anyhow::anyhow!("Clipboard / Schránka: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::io::Write;
        use std::process::Stdio;
        let uri =
            url::Url::from_file_path(path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;
        let mut c = if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            let mut c = command("wl-copy");
            c.args(["--type", "text/uri-list"]);
            c
        } else {
            let mut c = command("xclip");
            c.args(["-selection", "clipboard", "-t", "text/uri-list"]);
            c
        };
        let mut child = c
            .stdin(Stdio::piped())
            .spawn()
            .context("Install wl-clipboard or xclip / Nainstalujte wl-clipboard nebo xclip")?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{uri}\r\n").as_bytes())?;
        if !child.wait()?.success() {
            bail!("Clipboard operation failed / Operace schránky selhala");
        }
        Ok(())
    }
}
pub fn set_startup(enabled: bool, exe: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?
            .0;
        let value = format!("\"{}\" --background", exe.display());
        let old: Option<String> = key.get_value("WorkFlow Media").ok();
        if enabled {
            key.set_value("WorkFlow Media", &value)?;
        } else if old.as_deref() == Some(&value) {
            key.delete_value("WorkFlow Media")?;
        }
    }
    #[cfg(not(windows))]
    {
        let path = directories::BaseDirs::new()
            .context("No home directory")?
            .config_dir()
            .join("autostart/workflow-media.desktop");
        if enabled {
            let exe = exe
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('`', "\\`")
                .replace('$', "\\$")
                .replace('%', "%%");
            crate::model::atomic_write(&path,format!("[Desktop Entry]\nType=Application\nName=WorkFlow Media\nExec=\"{exe}\" --background\nTerminal=false\nX-GNOME-Autostart-enabled=true\n").as_bytes())?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
pub fn save_secret(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(windows)]
    {
        crate::model::atomic_write(path, &crypt(bytes, false)?)?;
    }
    #[cfg(not(windows))]
    {
        let key = keyring::Entry::new("workflow-media", &path.to_string_lossy())?;
        key.set_secret(bytes)?;
    }
    Ok(())
}
pub fn load_secret(path: &Path) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        crypt(&fs::read(path)?, true)
    }
    #[cfg(not(windows))]
    {
        Ok(keyring::Entry::new("workflow-media", &path.to_string_lossy())?.get_secret()?)
    }
}
pub fn delete_secret(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        if path.exists() {
            fs::remove_file(path)?;
        }
        let backup = PathBuf::from(format!("{}.bak", path.display()));
        if backup.exists() {
            fs::remove_file(backup)?;
        }
    }
    #[cfg(not(windows))]
    {
        match keyring::Entry::new("workflow-media", &path.to_string_lossy())?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
#[cfg(windows)]
fn crypt(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let result = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                1,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null(),
                1,
                &mut output,
            )
        }
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(bytes)
}
pub fn authorize_setup() -> Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
            UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
        };
        let exe = wide(std::env::current_exe()?);
        let verb = wide("runas");
        let parameters = wide("--authorize-setup");
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of_val(&info) as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.nShow = 0;
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            bail!("Administrator check cancelled / Ověření správce bylo zrušeno");
        }
        let mut exit = 1;
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            GetExitCodeProcess(info.hProcess, &mut exit);
            CloseHandle(info.hProcess);
        }
        if exit != 0 {
            bail!("Administrator check failed / Ověření správce selhalo");
        }
    }
    Ok(())
}
