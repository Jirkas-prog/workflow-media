use crate::model::tr;
use std::sync::mpsc::Sender;
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
#[derive(Debug, Clone, Copy)]
pub enum Action {
    Open,
    Paste,
    Clipboard,
    ToggleAuto,
    Pause,
    Folder,
    Settings,
    Quit,
}
pub struct Tray {
    #[cfg(windows)]
    _icon: TrayIcon,
    #[cfg(target_os = "linux")]
    _thread: std::thread::JoinHandle<()>,
    #[cfg(target_os = "linux")]
    language: Sender<String>,
}
fn make_menu(lang: &str) -> anyhow::Result<Menu> {
    let menu = Menu::new();
    let entries = [
        (
            "open",
            tr(lang, "Otevřít WorkFlow Media", "Open WorkFlow Media"),
            Action::Open,
        ),
        (
            "paste",
            tr(lang, "Vložit odkaz…", "Paste link…"),
            Action::Paste,
        ),
        (
            "clipboard",
            tr(lang, "Stáhnout ze schránky", "Download clipboard"),
            Action::Clipboard,
        ),
        (
            "auto",
            tr(
                lang,
                "Zapnout / vypnout sledování schránky",
                "Toggle clipboard monitoring",
            ),
            Action::ToggleAuto,
        ),
        (
            "pause",
            tr(lang, "Pozastavit / obnovit frontu", "Pause / resume queue"),
            Action::Pause,
        ),
        (
            "folder",
            tr(lang, "Otevřít složku", "Open download folder"),
            Action::Folder,
        ),
        (
            "settings",
            tr(lang, "Nastavení", "Settings"),
            Action::Settings,
        ),
        ("quit", tr(lang, "Ukončit", "Quit"), Action::Quit),
    ];
    for (i, (id, label, _)) in entries.iter().enumerate() {
        if i == 3 || i == 7 {
            menu.append(&PredefinedMenuItem::separator())?;
        }
        menu.append(&MenuItem::with_id(*id, *label, true, None))?;
    }
    Ok(menu)
}
fn build(
    lang: &str,
    tx: Sender<Action>,
    wake: crate::platform::WindowWake,
) -> anyhow::Result<TrayIcon> {
    let menu = make_menu(lang)?;
    let tx2 = tx.clone();
    let wake2 = wake.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = match event.id.0.as_str() {
            "open" => Action::Open,
            "paste" => Action::Paste,
            "clipboard" => Action::Clipboard,
            "auto" => Action::ToggleAuto,
            "pause" => Action::Pause,
            "folder" => Action::Folder,
            "settings" => Action::Settings,
            "quit" => Action::Quit,
            _ => return,
        };
        let _ = tx2.send(action);
        wake2.request();
    }));
    TrayIconEvent::set_event_handler(Some(move |e| {
        if matches!(
            e,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        ) {
            let _ = tx.send(Action::Open);
            wake.request();
        }
    }));
    let icon = image::load_from_memory(include_bytes!("../assets/workflow.png"))?
        .resize(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    Ok(TrayIconBuilder::new()
        .with_tooltip("WorkFlow Media")
        .with_icon(tray_icon::Icon::from_rgba(icon.into_raw(), 32, 32)?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?)
}
impl Tray {
    pub fn set_language(&mut self, lang: &str) -> anyhow::Result<()> {
        #[cfg(windows)]
        {
            self._icon.set_menu(Some(Box::new(make_menu(lang)?)));
        }
        #[cfg(target_os = "linux")]
        {
            self.language.send(lang.to_owned())?;
        }
        Ok(())
    }
    pub fn new(
        lang: &str,
        tx: Sender<Action>,
        wake: crate::platform::WindowWake,
    ) -> anyhow::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self {
                _icon: build(lang, tx, wake)?,
            })
        }
        #[cfg(target_os = "linux")]
        {
            let lang = lang.to_owned();
            let (ready, rx) = std::sync::mpsc::sync_channel(1);
            let (language, changes) = std::sync::mpsc::channel::<String>();
            let thread = std::thread::spawn(move || {
                let result = gtk::init()
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .and_then(|_| build(&lang, tx, wake));
                match result {
                    Ok(icon) => {
                        gtk::glib::timeout_add_local(
                            std::time::Duration::from_millis(100),
                            move || {
                                for lang in changes.try_iter() {
                                    if let Ok(menu) = make_menu(&lang) {
                                        icon.set_menu(Some(Box::new(menu)));
                                    }
                                }
                                gtk::glib::ControlFlow::Continue
                            },
                        );
                        let _ = ready.send(Ok(()));
                        gtk::main();
                    }
                    Err(e) => {
                        let _ = ready.send(Err(e));
                    }
                }
            });
            rx.recv()??;
            Ok(Self {
                _thread: thread,
                language,
            })
        }
    }
}
