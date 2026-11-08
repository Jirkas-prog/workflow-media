//! Small Windows update helper; media tools are downloaded with the main application.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]
#[path = "../force_update.rs"]
mod force_update;
#[path = "../maintenance.rs"]
mod maintenance;
#[path = "../model.rs"]
mod model;
#[path = "../platform.rs"]
mod platform;
#[path = "../updater.rs"]
mod updater;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version") {
        println!("WorkFlow Media Update {}", updater::VERSION);
        return;
    }
    #[cfg(windows)]
    let result = if args.iter().any(|a| a == "--headless") {
        force_update::run(&args)
    } else {
        progress(args.clone())
    };
    #[cfg(not(windows))]
    let result: anyhow::Result<()> = Err(anyhow::anyhow!("This update helper targets Windows x64"));
    if let Err(error) = result {
        eprintln!("{error:#}");
        #[cfg(windows)]
        if !args.iter().any(|a| a == "--headless") {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    platform::wide(format!("{error:#}")).as_ptr(),
                    platform::wide("WorkFlow Media Update").as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn progress(args: Vec<String>) -> anyhow::Result<()> {
    use std::sync::{Arc, Mutex};
    use windows_sys::Win32::{
        Foundation::*,
        UI::{Controls::*, WindowsAndMessaging::*},
    };
    struct Progress {
        args: Mutex<Option<Vec<String>>>,
        result: Mutex<Option<anyhow::Result<()>>>,
    }
    unsafe extern "system" fn callback(
        hwnd: HWND,
        notification: TASKDIALOG_NOTIFICATIONS,
        _: WPARAM,
        _: LPARAM,
        data: isize,
    ) -> i32 {
        let state = unsafe { &*(data as *const Arc<Progress>) };
        match notification {
            TDN_CREATED => {
                unsafe {
                    SendMessageW(hwnd, TDM_ENABLE_BUTTON as u32, IDOK as usize, 0);
                    SendMessageW(hwnd, TDM_SET_PROGRESS_BAR_MARQUEE as u32, 1, 40);
                }
                let args = state.args.lock().unwrap().take().unwrap();
                let worker = Arc::clone(state);
                std::thread::spawn(move || {
                    let result = force_update::run(&args);
                    worker.result.lock().unwrap().replace(result);
                });
            }
            TDN_TIMER => {
                let done = state.result.lock().unwrap().is_some();
                if done {
                    unsafe {
                        SendMessageW(hwnd, TDM_ENABLE_BUTTON as u32, IDOK as usize, 1);
                        SendMessageW(hwnd, TDM_CLICK_BUTTON as u32, IDOK as usize, 0);
                    }
                }
            }
            TDN_BUTTON_CLICKED => {
                if state.result.lock().unwrap().is_none() {
                    return 1;
                }
            }
            _ => {}
        }
        0
    }
    let data = args
        .windows(2)
        .find(|w| w[0] == "--data")
        .map(|w| std::path::PathBuf::from(&w[1]))
        .unwrap_or_else(platform::app_data);
    let cs = std::fs::read(data.join("queue.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .is_some_and(|v| v["Settings"]["Language"] == "cs");
    let title = platform::wide("WorkFlow Media Update");
    let heading = platform::wide(if cs {
        "Aktualizuji WorkFlow Media"
    } else {
        "Updating WorkFlow Media"
    });
    let text = platform::wide(if cs {
        "Uložím frontu, ověřím aktualizaci z GitHubu a znovu spustím aplikaci. Stahování bude pokračovat po restartu.\n\nVyčkejte prosím…"
    } else {
        "Saving the queue, checking the signed GitHub update and restarting the application. Downloads will continue after the restart.\n\nPlease wait…"
    });
    let state = Arc::new(Progress {
        args: Mutex::new(Some(args)),
        result: Mutex::new(None),
    });
    let mut config: TASKDIALOGCONFIG = unsafe { std::mem::zeroed() };
    config.cbSize = std::mem::size_of_val(&config) as u32;
    config.dwFlags = TDF_CALLBACK_TIMER | TDF_SHOW_MARQUEE_PROGRESS_BAR | TDF_SIZE_TO_CONTENT;
    config.dwCommonButtons = TDCBF_OK_BUTTON;
    config.pszWindowTitle = title.as_ptr();
    config.pszMainInstruction = heading.as_ptr();
    config.pszContent = text.as_ptr();
    config.pfCallback = Some(callback);
    config.lpCallbackData = &state as *const _ as isize;
    let hr = unsafe {
        TaskDialogIndirect(
            &config,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if hr < 0 {
        anyhow::bail!("Cannot show update progress / Nelze zobrazit průběh aktualizace");
    }
    state.result.lock().unwrap().take().unwrap_or_else(|| {
        Err(anyhow::anyhow!(
            "Update did not finish / Aktualizace nebyla dokončena"
        ))
    })
}
