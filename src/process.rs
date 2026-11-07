//! Supervise a helper and all its descendants, including yt-dlp's FFmpeg/Deno workers.
use anyhow::{Context, Result, bail};
use std::{
    io::{BufRead, BufReader, Read},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Tree {
    child: Child,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}
impl Tree {
    fn spawn(command: &mut Command) -> Result<Self> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::{io::AsRawHandle, process::CommandExt};
            use windows_sys::Win32::{
                Foundation::*,
                System::{JobObjects::*, Threading::*},
            };
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
            let mut child = command
                .spawn()
                .context("Cannot start media tool / Nelze spustit nástroj médií")?;
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(std::io::Error::last_os_error().into());
                }
                let tree = Self { child, job };
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const _) as _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                    || AssignProcessToJobObject(job, tree.child.as_raw_handle() as HANDLE) == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                // The process starts suspended: even the bootstrap child inherits the job.
                resume_primary_thread(tree.child.id())?;
                return Ok(tree);
            }
        }
        #[cfg(not(windows))]
        Ok(Self {
            child: command
                .spawn()
                .context("Cannot start media tool / Nelze spustit nástroj médií")?,
        })
    }
    fn terminate(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
        #[cfg(target_os = "linux")]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        self.terminate();
        let _ = self.child.wait();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}
#[cfg(windows)]
unsafe fn resume_primary_thread(pid: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::*,
        System::{Diagnostics::ToolHelp::*, Threading::*},
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut found = false;
        let mut next = Thread32First(snapshot, &mut entry);
        while next != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    found = ResumeThread(thread) != u32::MAX;
                    CloseHandle(thread);
                }
                break;
            }
            next = Thread32Next(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        if !found {
            bail!("Cannot resume media tool / Nelze spustit nástroj médií");
        }
        Ok(())
    }
}

pub fn run(
    command: &mut Command,
    cancel: &AtomicBool,
    timeout: Option<Duration>,
    mut output: impl FnMut(&str),
) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("@cancelled");
    }
    let mut tree = Tree::spawn(command)?;
    let (tx, rx) = mpsc::sync_channel(128);
    let mut readers = Vec::new();
    for (pipe, error) in [
        (
            Box::new(tree.child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            false,
        ),
        (
            Box::new(tree.child.stderr.take().unwrap()) as Box<dyn Read + Send>,
            true,
        ),
    ] {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let mut reader = BufReader::new(pipe);
            loop {
                let mut bytes = Vec::new();
                let count = (&mut reader)
                    .take(4 * 1024 * 1024)
                    .read_until(b'\n', &mut bytes);
                match count {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx
                            .send(Some((
                                error,
                                String::from_utf8_lossy(&bytes).trim().to_string(),
                            )))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            let _ = tx.send(None);
        }));
    }
    drop(tx);
    let started = Instant::now();
    let mut closed = 0;
    let mut status = None;
    let mut errors = std::collections::VecDeque::new();
    let result = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err(anyhow::anyhow!("@cancelled"));
        }
        if timeout.is_some_and(|t| started.elapsed() > t) {
            break Err(anyhow::anyhow!(
                "Media lookup timed out / Vyhledání videa vypršelo"
            ));
        }
        if status.is_none() {
            status = tree.child.try_wait()?;
            if status.is_some() {
                tree.terminate();
            }
        }
        match rx.recv_timeout(Duration::from_millis(75)) {
            Ok(Some((error, line))) => {
                if error {
                    if errors.len() == 12 {
                        errors.pop_front();
                    }
                    errors.push_back(line.chars().take(2048).collect::<String>());
                } else {
                    output(&line);
                }
            }
            Ok(None) => closed += 1,
            Err(mpsc::RecvTimeoutError::Disconnected) => closed = 2,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if closed >= 2 {
            if let Some(status) = status {
                break if status.success() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "Media tool failed / Nástroj médií selhal: {}",
                        errors.into_iter().collect::<Vec<_>>().join("\n")
                    ))
                };
            }
        }
    };
    tree.terminate();
    drop(rx); // Release producers blocked on a full channel before joining them.
    drop(tree);
    for reader in readers {
        let _ = reader.join();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_terminates_descendants_and_closes_pipes() {
        let cancel = AtomicBool::new(false);
        #[cfg(windows)]
        let mut cmd = {
            let mut c = crate::platform::command("powershell.exe");
            c.args([
                "-NoProfile",
                "-Command",
                "& ping.exe -n 60 127.0.0.1 | Out-Null",
            ]);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = crate::platform::command("sh");
            c.args(["-c", "sleep 60 & wait"]);
            c
        };
        let started = Instant::now();
        std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(800));
                cancel.store(true, Ordering::Relaxed);
            });
            assert_eq!(
                run(&mut cmd, &cancel, None, |_| {})
                    .unwrap_err()
                    .to_string(),
                "@cancelled"
            );
        });
        assert!(started.elapsed() < Duration::from_secs(8));
    }
}
