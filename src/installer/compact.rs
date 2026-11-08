//! A small native progress window for the email installer; the main UI remains egui.
use super::*;
use std::sync::Mutex;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::{
        Controls::*,
        WindowsAndMessaging::{
            IDCANCEL, MB_ICONERROR, MB_ICONWARNING, MB_OK, MessageBoxW, PostMessageW, SendMessageW,
        },
    },
};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

pub fn message(text: &str, error: bool) {
    let title = wide("WorkFlow Media Setup");
    let text = wide(text);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | if error { MB_ICONERROR } else { MB_ICONWARNING },
        );
    }
}

struct Dialog {
    language: String,
    ready: Mutex<Option<mpsc::Sender<()>>>,
    events: Mutex<mpsc::Receiver<Event>>,
    result: Mutex<Option<Result<Installed>>>,
    cancel: Arc<AtomicBool>,
    done: AtomicBool,
}

fn content(window: HWND, text: &str) {
    let text = wide(text);
    // The string remains alive until this synchronous message returns.
    unsafe {
        SendMessageW(
            window,
            TDM_SET_ELEMENT_TEXT as u32,
            TDE_CONTENT as usize,
            text.as_ptr() as isize,
        );
    }
}

unsafe extern "system" fn callback(
    window: HWND,
    notification: TASKDIALOG_NOTIFICATIONS,
    _wparam: WPARAM,
    _lparam: LPARAM,
    data: isize,
) -> windows_sys::core::HRESULT {
    // TaskDialogIndirect is synchronous. State outlives the dialog and the worker.
    // Use shared access: SendMessage can re-enter this callback.
    let state = unsafe { &*(data as *const Dialog) };
    match notification {
        TDN_CREATED => {
            unsafe {
                SendMessageW(window, TDM_SET_PROGRESS_BAR_MARQUEE as u32, 1, 30);
            }
            if let Some(ready) = state.ready.lock().unwrap().take() {
                let _ = ready.send(());
            }
        }
        TDN_TIMER if !state.done.load(Ordering::Relaxed) => loop {
            let event = state.events.lock().unwrap().try_recv();
            match event {
                Ok(Event::Stage(stage)) => {
                    if !state.cancel.load(Ordering::Relaxed) {
                        content(window, stage.label(&state.language));
                    }
                }
                Ok(Event::Done(result)) => {
                    *state.result.lock().unwrap() = Some(result);
                    state.done.store(true, Ordering::Relaxed);
                    unsafe {
                        PostMessageW(window, TDM_CLICK_BUTTON as u32, IDCANCEL as usize, 0);
                    }
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    *state.result.lock().unwrap() =
                        Some(Err(anyhow::anyhow!("Installation worker stopped")));
                    state.done.store(true, Ordering::Relaxed);
                    unsafe {
                        PostMessageW(window, TDM_CLICK_BUTTON as u32, IDCANCEL as usize, 0);
                    }
                    break;
                }
            }
        },
        TDN_BUTTON_CLICKED if !state.done.load(Ordering::Relaxed) => {
            state.cancel.store(true, Ordering::Relaxed);
            content(
                window,
                tr(
                    &state.language,
                    "Ukončuji instalaci…",
                    "Stopping installation…",
                ),
            );
            return 1; // Keep the window alive while the worker finishes cleanup.
        }
        _ => {}
    }
    0
}

pub fn run(profile: Profile) -> Result<()> {
    let (sender, events) = mpsc::channel();
    let (ready_sender, ready_receiver) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let state = Dialog {
        language: profile.choices.language.clone(),
        ready: Mutex::new(Some(ready_sender)),
        events: Mutex::new(events),
        result: Mutex::new(None),
        cancel: cancel.clone(),
        done: AtomicBool::new(false),
    };
    let worker = std::thread::spawn(move || {
        // Do not install invisibly if Windows cannot create the progress dialog.
        if ready_receiver.recv().is_ok() {
            let result = install(&profile, None, &cancel, |stage| {
                let _ = sender.send(Event::Stage(stage));
            });
            let _ = sender.send(Event::Done(result));
        }
    });
    let title = wide("WorkFlow Media Setup");
    let heading = wide(tr(
        &state.language,
        "Instalace WorkFlow Media",
        "Installing WorkFlow Media",
    ));
    let status = wide(Stage::Authorize.label(&state.language));
    let cancel_text = wide(tr(&state.language, "Zrušit", "Cancel"));
    let button = TASKDIALOG_BUTTON {
        nButtonID: IDCANCEL,
        pszButtonText: cancel_text.as_ptr(),
    };
    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        dwFlags: TDF_CALLBACK_TIMER
            | TDF_ALLOW_DIALOG_CANCELLATION
            | TDF_SHOW_MARQUEE_PROGRESS_BAR
            | TDF_CAN_BE_MINIMIZED,
        pszWindowTitle: title.as_ptr(),
        pszMainInstruction: heading.as_ptr(),
        pszContent: status.as_ptr(),
        cButtons: 1,
        pButtons: &button,
        pfCallback: Some(callback),
        lpCallbackData: &state as *const Dialog as isize,
        cxWidth: 300,
        ..unsafe { std::mem::zeroed() }
    };
    let result = unsafe {
        TaskDialogIndirect(
            &config,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    state.ready.lock().unwrap().take();
    if result < 0 {
        state.cancel.store(true, Ordering::Relaxed);
    }
    worker
        .join()
        .map_err(|_| anyhow::anyhow!("Installation worker stopped"))?;
    if result < 0 {
        bail!(
            "Cannot open installer / Nelze otevřít instalátor (0x{:08X})",
            result as u32
        );
    }
    if state.cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    let installed = state
        .result
        .into_inner()
        .unwrap()
        .context("Installation did not finish")??;
    if !installed.warning.is_empty() {
        message(&installed.warning, false);
    }
    Ok(())
}
