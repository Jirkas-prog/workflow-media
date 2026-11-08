use crate::{
    engine::Engine,
    model::{self, ClipboardMode, Job, JobState, Preset, Settings, State, tr},
    platform,
    style::{self, BLUE, GREEN, LIME, MUTED, RED},
    tray::{self, Action},
    updater,
};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText, Vec2};
use egui_extras::{Column, TableBuilder};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Downloads,
    FastShare,
    Library,
    Settings,
}
#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Newest,
    Oldest,
    NameAsc,
    NameDesc,
    SourceAsc,
    SourceDesc,
    StateAsc,
    StateDesc,
    ProgressAsc,
    ProgressDesc,
    SpeedAsc,
    SpeedDesc,
}
#[derive(Default)]
struct Filter {
    query: String,
    source: String,
    state: Option<JobState>,
}
enum Event {
    Cookies(Result<crate::cookies::RefreshReport>, bool, u64),
    Done(Result<String>),
    Notice(Result<String>),
    Search(Result<Vec<crate::fastshare::SearchResult>>),
    Update(Result<Option<updater::Manifest>>),
    Downloaded(Result<PathBuf>),
    Exported(Result<PathBuf>),
    Stopped,
}
enum Confirm {
    Clear(HashSet<String>, bool),
    Recycle(String),
    Quit,
}
enum FileAction {
    Open(Job),
    Folder(Job),
    Info(Job),
    CopyFile(Job),
    CopyPath(Job),
    Source(Job),
    Transfer(Job, bool),
    Recycle(Job),
    Remove(Job, bool),
}
pub struct App {
    engine: Engine,
    settings: Settings,
    state: State,
    page: Page,
    preset: PathBuf,
    data: PathBuf,
    exe: PathBuf,
    setup: Option<usize>,
    preview: bool,
    builder: bool,
    builder_folder: String,
    install_location: crate::installer::Location,
    created_installer: Option<PathBuf>,
    setup_saved: Option<Settings>,
    cookies_busy: bool,
    cookie_epoch: u64,
    cookie_cancel: Arc<AtomicBool>,
    cookie_task: Option<std::thread::JoinHandle<()>>,
    cookie_results: Vec<(String, std::result::Result<usize, String>)>,
    links: String,
    format: String,
    quality: i32,
    destination: String,
    selected: HashSet<String>,
    anchor: Option<usize>,
    filter: Filter,
    sort: Sort,
    notice: String,
    busy: usize,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    tray_rx: Receiver<Action>,
    tray: Option<tray::Tray>,
    stop: Arc<AtomicBool>,
    monitor_ready: Arc<AtomicBool>,
    snapshot_at: Instant,
    show_signal: Option<String>,
    confirm: Option<Confirm>,
    info: Option<Job>,
    paste_dialog: bool,
    search: String,
    search_results: Vec<crate::fastshare::SearchResult>,
    login: String,
    password: String,
    update: Option<updater::Manifest>,
    update_file: Option<PathBuf>,
    update_auto: bool,
    update_checked: Instant,
    quitting: bool,
    exit_ready: bool,
}
impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        engine: Engine,
        preset: PathBuf,
        data: PathBuf,
        setup: bool,
        preview: bool,
        background: bool,
        initial_signal: Option<String>,
        builder: bool,
    ) -> Self {
        style::apply(&cc.egui_ctx);
        let state = engine.snapshot();
        let settings = state.settings.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let (tray_tx, tray_rx) = std::sync::mpsc::channel();
        let wake = platform::WindowWake::new(cc);
        let tray = if builder {
            Ok(None)
        } else {
            tray::Tray::new(&settings.language, tray_tx.clone(), wake.clone()).map(Some)
        };
        let notice = tray
            .as_ref()
            .err()
            .map(|e| format!("Tray / Ikona u hodin: {e}"))
            .unwrap_or_default();
        let tray = tray.ok().flatten();
        if background && !setup && tray.is_some() {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let monitor_ready = Arc::new(AtomicBool::new(!setup && !preview));
        let mut app = Self {
            engine: engine.clone(),
            format: settings.format.clone(),
            quality: settings.quality,
            destination: settings.download_directory.clone(),
            settings,
            state,
            page: Page::Downloads,
            preset,
            data,
            exe: std::env::current_exe().unwrap_or_default(),
            setup: setup.then_some(0),
            preview,
            builder,
            builder_folder: "WorkFlow Media".into(),
            install_location: crate::installer::Location::default(),
            created_installer: None,
            setup_saved: None,
            cookies_busy: false,
            cookie_epoch: 0,
            cookie_cancel: Arc::new(AtomicBool::new(false)),
            cookie_task: None,
            cookie_results: Vec::new(),
            links: String::new(),
            selected: HashSet::new(),
            anchor: None,
            filter: Filter::default(),
            sort: Sort::Newest,
            notice,
            busy: 0,
            tx,
            rx,
            tray_rx,
            tray,
            stop: stop.clone(),
            monitor_ready: monitor_ready.clone(),
            snapshot_at: Instant::now() - Duration::from_secs(1),
            show_signal: initial_signal,
            confirm: None,
            info: None,
            paste_dialog: false,
            search: String::new(),
            search_results: vec![],
            login: String::new(),
            password: String::new(),
            update: None,
            update_file: None,
            update_auto: false,
            update_checked: Instant::now() - Duration::from_secs(86401),
            quitting: false,
            exit_ready: false,
        };
        if !setup && !preview {
            engine.start();
        }
        let tx = app.tx.clone();
        let ctx = cc.egui_ctx.clone();
        if !builder {
            std::thread::spawn(move || {
                let mut clipboard = arboard::Clipboard::new().ok();
                let mut last = clipboard
                    .as_mut()
                    .and_then(|c| c.get_text().ok())
                    .unwrap_or_default();
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(400));
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    // Process tray actions, second launches and updates even while hidden.
                    wake.request();
                    if clipboard.is_none() {
                        clipboard = arboard::Clipboard::new().ok();
                    }
                    let Some(text) = clipboard.as_mut().and_then(|c| c.get_text().ok()) else {
                        continue;
                    };
                    if text == last {
                        continue;
                    }
                    last = text.clone();
                    if !monitor_ready.load(Ordering::Relaxed) {
                        continue;
                    }
                    let s = engine.snapshot().settings;
                    let links = model::clipboard_links(&text, &s);
                    if links.is_empty() {
                        continue;
                    }
                    let result = engine.add_presets(&links).map(|n| {
                        if n > 0 {
                            format!(
                                "{}: {n}",
                                tr(&s.language, "Přidáno ze schránky", "Added from clipboard")
                            )
                        } else {
                            String::new()
                        }
                    });
                    let _ = tx.send(Event::Notice(result));
                    ctx.request_repaint();
                }
            });
        }
        if app.preview {
            app.notice = tr(
                &app.settings.language,
                "Náhled · nastavení se neuloží",
                "Preview · settings will not be saved",
            )
            .into();
        }
        app
    }
    fn t<'a>(&self, cs: &'a str, en: &'a str) -> &'a str {
        tr(&self.settings.language, cs, en)
    }
    fn spawn(&mut self, ctx: &egui::Context, f: impl FnOnce() -> Result<String> + Send + 'static) {
        self.busy += 1;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Done(f()));
            ctx.request_repaint();
        });
    }
    fn result(&mut self, result: Result<impl ToString>) {
        self.notice = match result {
            Ok(v) => v.to_string(),
            Err(e) => model::localized_error(&self.settings.language, &format!("{e:#}")),
        };
    }
    fn save(&mut self) -> Result<()> {
        if self.cookies_busy {
            anyhow::bail!(
                "Wait for browser profiles to finish loading / Počkejte na dokončení načítání profilů prohlížečů"
            );
        }
        self.settings.validate()?;
        if self.preview {
            return Ok(());
        }
        let old = self.engine.snapshot().settings;
        let previous_preset = std::fs::read(&self.preset).ok();
        // Persist the preset first, then roll back both settings and OS registration on failure.
        model::save_json(
            &self.preset,
            &Preset {
                setup_completed: true,
                settings: self.settings.clone(),
                ..Preset::default()
            },
        )?;
        let saved = platform::set_startup(self.settings.start_with_windows, &self.exe)
            .and_then(|_| self.engine.settings(self.settings.clone()));
        if let Err(e) = saved {
            let _ = platform::set_startup(old.start_with_windows, &self.exe);
            if let Some(bytes) = previous_preset {
                let _ = model::atomic_write(&self.preset, &bytes);
            } else {
                let _ = std::fs::remove_file(&self.preset);
            }
            return Err(e);
        }
        self.format = self.settings.format.clone();
        self.quality = self.settings.quality;
        self.destination = self.settings.download_directory.clone();
        Ok(())
    }
    fn save_columns(&mut self) {
        if !self.preview {
            let mut settings = self.engine.snapshot().settings;
            settings.hidden_columns = self.settings.hidden_columns.clone();
            let preset = Preset {
                setup_completed: self.setup.is_none(),
                settings: settings.clone(),
                ..Preset::default()
            };
            let result = model::save_json(&self.preset, &preset)
                .and_then(|_| self.engine.settings(settings));
            if let Err(e) = result {
                self.notice = model::localized_error(&self.settings.language, &e.to_string());
            }
        }
    }
    fn show(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    fn switch(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.selected.clear();
            self.filter = Filter::default();
            self.sort = Sort::Newest;
        }
    }
    fn clipboard(&mut self) -> Option<String> {
        match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(s) => Some(s),
            Err(e) => {
                self.notice = format!(
                    "{}: {e}",
                    self.t("Schránku nelze přečíst", "Cannot read clipboard")
                );
                None
            }
        }
    }
    fn add_links(&mut self) {
        let result = self.engine.add(
            &self.links,
            &self.format,
            self.quality,
            &self.destination,
            self.settings.start_on_add,
        );
        match result {
            Ok((n, d)) => {
                self.notice = format!(
                    "{}: {n} · {}: {d}",
                    self.t("Přidáno", "Added"),
                    self.t("Duplicitní odkazy", "Duplicates")
                );
                self.links.clear();
                self.paste_dialog = false;
            }
            Err(e) => self.notice = model::localized_error(&self.settings.language, &e.to_string()),
        }
    }
    fn quit(&mut self, ctx: &egui::Context) {
        if self.quitting {
            return;
        }
        self.quitting = true;
        self.invalidate_cookie_request();
        let cookie_task = self.cookie_task.take();
        self.monitor_ready.store(false, Ordering::Relaxed);
        let engine = self.engine.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            engine.shutdown();
            // Let temporary cookie files be removed before the process exits.
            if let Some(task) = cookie_task {
                let _ = task.join();
            }
            let _ = tx.send(Event::Stopped);
            ctx.request_repaint();
        });
    }
    fn events(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                Action::Open => self.show(ctx),
                Action::Paste => {
                    self.show(ctx);
                    self.paste_dialog = true;
                    if let Some(s) = self.clipboard() {
                        self.links = s;
                    }
                }
                Action::Clipboard => {
                    if let Some(s) = self.clipboard() {
                        let mut settings = self.settings.clone();
                        settings.clipboard_mode = ClipboardMode::All;
                        let links = model::clipboard_links(&s, &settings);
                        if links.is_empty() {
                            self.notice = self
                                .t(
                                    "Schránka neobsahuje samostatný odkaz",
                                    "Clipboard contains no standalone links",
                                )
                                .into();
                        } else {
                            let r = self
                                .engine
                                .add_presets(&links)
                                .map(|n| format!("{}: {n}", self.t("Přidáno", "Added")));
                            self.result(r);
                        }
                    }
                }
                Action::ToggleAuto => {
                    self.settings.clipboard_mode =
                        if self.settings.clipboard_mode == ClipboardMode::Off {
                            ClipboardMode::Selected
                        } else {
                            ClipboardMode::Off
                        };
                    let r = self.save().map(|_| {
                        self.t("Nastavení schránky uloženo", "Clipboard settings saved")
                            .to_string()
                    });
                    self.result(r);
                }
                Action::Pause => self.engine.toggle_queue(),
                Action::Folder => {
                    let r = open::that(&self.settings.download_directory).map(|_| "");
                    self.result(r.map_err(Into::into));
                }
                Action::Settings => {
                    self.switch(Page::Settings);
                    self.show(ctx);
                }
                Action::Quit => {
                    self.show(ctx);
                    self.confirm = Some(Confirm::Quit);
                }
            }
        }
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Cookies(result, refreshed, epoch) => {
                    self.cookies_busy = false;
                    self.busy = self.busy.saturating_sub(1);
                    if self.cookie_epoch != epoch {
                        continue;
                    }
                    match result {
                        Ok(report) => {
                            self.settings.cookie_profiles = crate::cookies::merge_profiles(
                                report.profiles,
                                &self.settings.cookie_profiles,
                            );
                            self.cookie_results = report.results;
                            let successes = self
                                .cookie_results
                                .iter()
                                .filter(|(_, r)| r.is_ok())
                                .count();
                            self.notice = if refreshed {
                                format!(
                                    "{} {successes}/{}. {}",
                                    self.t("Cookies načteny:", "Cookies loaded:"),
                                    self.cookie_results.len(),
                                    self.t(
                                        "Volby potvrďte uložením nastavení.",
                                        "Save settings to apply your choices."
                                    )
                                )
                            } else {
                                format!(
                                    "{} {}",
                                    self.t("Nalezené profily:", "Profiles found:"),
                                    self.settings.cookie_profiles.len()
                                )
                            };
                        }
                        Err(e) => self.result(Err::<String, _>(e)),
                    }
                }
                Event::Exported(result) => {
                    self.busy = self.busy.saturating_sub(1);
                    match result {
                        Ok(path) => {
                            self.notice = self
                                .t(
                                    "Instalační soubor je připravený k předání.",
                                    "The installer is ready to share.",
                                )
                                .into();
                            self.created_installer = Some(path);
                        }
                        Err(e) => self.result(Err::<String, _>(e)),
                    }
                }
                Event::Notice(result) => match result {
                    Ok(s) if s.is_empty() => {}
                    other => self.result(other),
                },
                Event::Done(result) => {
                    self.busy = self.busy.saturating_sub(1);
                    match result {
                        Ok(s) if s.is_empty() => {}
                        other => self.result(other),
                    }
                }
                Event::Search(result) => {
                    self.busy = self.busy.saturating_sub(1);
                    match result {
                        Ok(rows) => {
                            self.notice =
                                format!("{}: {}", self.t("Nalezeno", "Found"), rows.len());
                            self.search_results = rows;
                        }
                        Err(e) => {
                            self.notice =
                                model::localized_error(&self.settings.language, &e.to_string())
                        }
                    }
                }
                Event::Update(result) => {
                    self.busy = self.busy.saturating_sub(1);
                    match result {
                        Ok(Some(m)) => {
                            self.notice = format!(
                                "{} {}",
                                self.t("Dostupná verze", "Available version"),
                                m.version
                            );
                            self.update = Some(m);
                        }
                        Ok(None) => {
                            self.notice = self.t("Máte aktuální verzi", "You are up to date").into()
                        }
                        Err(e) => {
                            self.notice = format!(
                                "{}: {e}",
                                self.t("Aktualizaci nelze ověřit", "Cannot check updates")
                            );
                            self.update_auto = false;
                        }
                    }
                }
                Event::Downloaded(result) => {
                    self.busy = self.busy.saturating_sub(1);
                    match result {
                        Ok(path) => {
                            self.update_file = Some(path);
                            self.notice = self
                                .t(
                                    "Aktualizace ověřena. Připraveno k restartu.",
                                    "Update verified. Ready to restart.",
                                )
                                .into();
                        }
                        Err(e) => {
                            self.notice =
                                model::localized_error(&self.settings.language, &e.to_string());
                            self.update_auto = false;
                        }
                    }
                }
                Event::Stopped => {
                    if let Some(path) = self.update_file.take() {
                        if let Err(e) =
                            updater::launch_install(&path, &self.exe, &self.data, &self.preset)
                        {
                            self.notice =
                                model::localized_error(&self.settings.language, &e.to_string());
                            self.quitting = false;
                            self.engine.start();
                            self.monitor_ready.store(true, Ordering::Relaxed);
                            self.update_auto = false;
                            continue;
                        }
                    }
                    self.exit_ready = true;
                    self.stop.store(true, Ordering::Relaxed);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        if self.snapshot_at.elapsed() > Duration::from_millis(100) {
            self.state = self.engine.snapshot();
            self.snapshot_at = Instant::now();
            let notice = self.engine.notice();
            if !notice.is_empty() {
                self.notice = notice;
            }
            let signal = std::fs::read_to_string(self.data.join("show.signal")).ok();
            if signal.is_some() && signal != self.show_signal {
                self.show(ctx);
            }
            self.show_signal = signal;
        }
        if self.setup.is_none()
            && !self.preview
            && !self.quitting
            && self.settings.automatic_updates
            && self.update_checked.elapsed() > Duration::from_secs(86400)
        {
            self.check_update(ctx, true);
        }
        let idle = !self
            .state
            .jobs
            .iter()
            .any(|j| j.state.active() || j.state == JobState::Queued);
        if self.update_auto && self.busy == 0 && idle {
            if self.update_file.is_some() {
                self.quit(ctx);
            } else if self.update.is_some() {
                self.download_update(ctx);
            }
        }
    }
    fn check_update(&mut self, ctx: &egui::Context, automatic: bool) {
        self.update_checked = Instant::now();
        self.update_auto = automatic;
        self.busy += 1;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Update(updater::check()));
            ctx.request_repaint();
        });
    }
    fn download_update(&mut self, ctx: &egui::Context) {
        let Some(m) = self.update.clone() else { return };
        self.busy += 1;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let cache = self.data.join("updates");
        std::thread::spawn(move || {
            let _ = tx.send(Event::Downloaded(updater::download(&m, &cache)));
            ctx.request_repaint();
        });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation")
            .exact_width(202.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(15, 30, 39))
                    .inner_margin(20),
            )
            .show(ctx, |ui| {
                ui.add_space(10.);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(38., 42.), egui::Sense::hover());
                style::logo(ui.painter(), rect.center(), 29., LIME);
                ui.label(RichText::new("WorkFlow").size(28.).strong());
                ui.label(RichText::new("M E D I A").size(12.).color(LIME));
                ui.add_space(33.);
                for (page, cs, en, icon) in [
                    (
                        Page::Downloads,
                        "Stahování",
                        "Downloads",
                        style::NavIcon::Download,
                    ),
                    (
                        Page::FastShare,
                        "FastShare",
                        "FastShare",
                        style::NavIcon::Search,
                    ),
                    (
                        Page::Library,
                        "Knihovna",
                        "Library",
                        style::NavIcon::Library,
                    ),
                    (
                        Page::Settings,
                        "Nastavení",
                        "Settings",
                        style::NavIcon::Settings,
                    ),
                ] {
                    if style::nav_button(ui, self.t(cs, en), icon, self.page == page).clicked() {
                        self.switch(page);
                    }
                    ui.add_space(5.);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    style::caption(
                        ui,
                        &format!(
                            "v{} · Rust · {}",
                            updater::VERSION,
                            if cfg!(windows) { "Windows" } else { "Linux" }
                        ),
                    );
                    ui.label(
                        RichText::new(self.t(
                            "Nejvyšší kvalita.\nPod vaší kontrolou.",
                            "Highest quality.\nYour choice.",
                        ))
                        .color(MUTED)
                        .size(13.),
                    );
                    ui.add_space(8.);
                    let label = if self.state.settings.queue_paused {
                        self.t("▶  Obnovit frontu", "▶  Resume queue")
                    } else {
                        self.t("Ⅱ  Pozastavit frontu", "Ⅱ  Pause queue")
                    };
                    if ui.button(label).clicked() {
                        self.engine.toggle_queue();
                    }
                    ui.add_space(15.);
                });
            });
    }
    fn input_card(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        style::card().inner_margin(14).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 7.;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr(&lang, "ODKAZY KE STAŽENÍ", "DOWNLOAD LINKS"))
                        .size(12.)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(tr(&lang, "Vložit schránku", "Paste clipboard"))
                        .clicked()
                    {
                        if let Some(s) = self.clipboard() {
                            self.links = s;
                        }
                    }
                });
            });
            ui.add(
                style::text_area(&mut self.links)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .hint_text(tr(
                        &lang,
                        "Vložte jeden nebo více odkazů, každý na vlastní řádek…",
                        "Paste one or more links, one per line…",
                    )),
            );
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    style::caption(ui, tr(&lang, "Formát", "Format"));
                    format_combo(ui, "add-format", &mut self.format, false, &lang);
                });
                ui.vertical(|ui| {
                    style::caption(ui, tr(&lang, "Kvalita", "Quality"));
                    quality_combo(ui, "add-quality", &mut self.quality, false, &lang);
                });
                ui.vertical(|ui| {
                    style::caption(ui, tr(&lang, "Uložit do", "Save to"));
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 205.).max(100.);
                        style::wrapping_field(ui, &mut self.destination, width, "");
                        if ui
                            .button("…")
                            .on_hover_text(tr(&lang, "Vybrat složku", "Choose folder"))
                            .clicked()
                        {
                            if let Some(p) = rfd::FileDialog::new()
                                .set_directory(&self.destination)
                                .pick_folder()
                            {
                                self.destination = p.to_string_lossy().into();
                            }
                        }
                        if style::primary(ui, tr(&lang, "+  Přidat do fronty", "+  Add to queue"))
                            .clicked()
                        {
                            self.add_links();
                        }
                    });
                });
            });
        });
    }
    fn metrics(&self, ui: &mut egui::Ui) {
        let active = self.state.jobs.iter().filter(|j| j.state.active()).count();
        let queued = self
            .state
            .jobs
            .iter()
            .filter(|j| j.state == JobState::Queued)
            .count();
        let done = self.state.jobs.iter().filter(|j| j.in_library()).count();
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 66.), egui::Sense::hover());
        let p = ui.painter();
        p.rect(
            rect,
            10.,
            style::PANEL,
            egui::Stroke::new(1.0_f32, style::LINE),
            egui::StrokeKind::Inside,
        );
        let width = rect.width() / 3.;
        let mut accessible = Vec::new();
        for (i, (label, n, color)) in [
            (self.t("Právě se stahuje", "Downloading"), active, LIME),
            (self.t("Ve frontě", "Queued"), queued, BLUE),
            (self.t("Dokončeno", "Completed"), done, GREEN),
        ]
        .iter()
        .enumerate()
        {
            let x = rect.left() + i as f32 * width;
            if i > 0 {
                p.line_segment(
                    [
                        egui::pos2(x, rect.top() + 15.),
                        egui::pos2(x, rect.bottom() - 15.),
                    ],
                    egui::Stroke::new(1.0_f32, style::LINE),
                );
            }
            let name = p.layout_no_wrap(
                format!("{label}:"),
                egui::FontId::proportional(18.),
                style::TEXT,
            );
            let number = p.layout_no_wrap(n.to_string(), egui::FontId::proportional(27.), *color);
            let name_width = name.size().x;
            let start = x + (width - name.size().x - number.size().x - 12.) / 2.;
            p.galley(
                egui::pos2(start, rect.center().y - name.size().y / 2.),
                name,
                style::TEXT,
            );
            p.galley(
                egui::pos2(
                    start + name_width + 12.,
                    rect.center().y - number.size().y / 2.,
                ),
                number,
                *color,
            );
            accessible.push(format!("{label}: {n}"));
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Label,
                ui.is_enabled(),
                accessible.join(", "),
            )
        });
    }
    fn downloads(&mut self, ui: &mut egui::Ui, library: bool) {
        if !library {
            self.input_card(ui);
            ui.add_space(8.);
            self.metrics(ui);
            ui.add_space(8.);
        }
        style::card()
            .fill(style::PANEL)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                self.files_table(ui, library);
            });
    }
    fn files_table(&mut self, ui: &mut egui::Ui, library: bool) {
        let lang = self.settings.language.clone();
        ui.horizontal(|ui| {
            style::title(
                ui,
                tr(
                    &lang,
                    if library {
                        "Knihovna"
                    } else {
                        "Přehled souborů"
                    },
                    if library { "Library" } else { "Files" },
                ),
            );
            if !self.filter.query.is_empty()
                || !self.filter.source.is_empty()
                || self.filter.state.is_some()
            {
                if ui
                    .small_button(tr(&lang, "Zrušit filtry ×", "Clear filters ×"))
                    .clicked()
                {
                    self.filter = Filter::default();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(tr(&lang, "Sloupce ▾", "Columns ▾"), |ui| {
                    for (id, cs, en) in [
                        ("Source", "Zdroj", "Source"),
                        ("State", "Stav", "Status"),
                        ("Progress", "Průběh", "Progress"),
                        ("Speed", "Rychlost", "Speed"),
                    ] {
                        let mut show = !self
                            .settings
                            .hidden_columns
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(id));
                        if style::checkbox(ui, &mut show, tr(&lang, cs, en)).changed() {
                            self.settings
                                .hidden_columns
                                .retain(|s| !s.eq_ignore_ascii_case(id));
                            if !show {
                                self.settings.hidden_columns.push(id.to_lowercase());
                            }
                            self.save_columns();
                        }
                    }
                });
            });
        });
        let all: Vec<Job> = self
            .state
            .jobs
            .iter()
            .filter(|j| {
                if library {
                    j.in_library()
                } else {
                    !j.hidden_from_queue
                }
            })
            .cloned()
            .collect();
        let mut jobs: Vec<Job> = all
            .iter()
            .filter(|j| {
                (self.filter.query.is_empty()
                    || j.title
                        .to_lowercase()
                        .contains(&self.filter.query.to_lowercase()))
                    && (self.filter.source.is_empty() || j.source == self.filter.source)
                    && self.filter.state.is_none_or(|s| j.state == s)
            })
            .cloned()
            .collect();
        sort_jobs(&mut jobs, self.sort);
        ui.add_space(3.);
        let visible_cols: Vec<&str> = ["Source", "State", "Progress", "Speed"]
            .into_iter()
            .filter(|id| {
                !self
                    .settings
                    .hidden_columns
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(id))
            })
            .collect();
        let mut actions = vec![];
        let footer_height = if ui.available_width() < 850. && !library {
            157.
        } else {
            122.
        };
        let height = (ui.available_height() - footer_height).max(64.);
        let header_rect =
            egui::Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 36.));
        ui.painter()
            .rect_filled(header_rect, 4., Color32::from_rgb(16, 33, 42));
        ui.painter().line_segment(
            [header_rect.left_bottom(), header_rect.right_bottom()],
            egui::Stroke::new(1.0_f32, style::LINE),
        );
        let mut table = TableBuilder::new(ui)
            .id_salt(if library {
                "library-table"
            } else {
                "queue-table"
            })
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .sense(egui::Sense::click())
            .min_scrolled_height(height)
            .max_scroll_height(height)
            .column(Column::remainder().at_least(180.));
        for id in &visible_cols {
            table = table.column(
                Column::initial(match *id {
                    "Source" => 104.,
                    "State" => 155.,
                    _ => 104.,
                })
                .range(70.0..=300.0),
            );
        }
        table
            .header(36., |mut row| {
                row.col(|ui| self.header(ui, "Name", &all));
                for id in &visible_cols {
                    row.col(|ui| self.header(ui, id, &all));
                }
            })
            .body(|body| {
                body.rows(35., jobs.len(), |mut row| {
                    let index = row.index();
                    let job = &jobs[index];
                    row.set_selected(self.selected.contains(&job.id));
                    row.col(|ui| {
                        ui.add(egui::Label::new(&job.title).truncate())
                            .on_hover_text(&job.title);
                    });
                    for id in &visible_cols {
                        row.col(|ui| match *id {
                            "Source" => {
                                ui.label(source_label(&job.source, &lang));
                            }
                            "State" => {
                                ui.colored_label(state_color(job.state), job.state.label(&lang));
                            }
                            "Progress" => {
                                if job.state == JobState::Completed {
                                    ui.label("100 %");
                                } else if job.state.active() {
                                    ui.add(
                                        egui::ProgressBar::new((job.percent / 100.) as f32)
                                            .desired_width(ui.available_width())
                                            .show_percentage()
                                            .fill(LIME),
                                    );
                                } else {
                                    ui.label("—");
                                }
                            }
                            "Speed" => {
                                ui.label(if job.speed > 0. {
                                    format!("{}/s", model::bytes(job.speed))
                                } else {
                                    "—".into()
                                });
                            }
                            _ => {}
                        });
                    }
                    let response = row.response();
                    if response.clicked() {
                        let modifiers = response.ctx.input(|i| i.modifiers);
                        if modifiers.shift {
                            let anchor = self.anchor.unwrap_or(index);
                            for j in
                                &jobs[anchor.min(index)..=anchor.max(index).min(jobs.len() - 1)]
                            {
                                self.selected.insert(j.id.clone());
                            }
                        } else if modifiers.command {
                            if !self.selected.remove(&job.id) {
                                self.selected.insert(job.id.clone());
                            }
                            self.anchor = Some(index);
                        } else {
                            self.selected.clear();
                            self.selected.insert(job.id.clone());
                            self.anchor = Some(index);
                        }
                    }
                    if response.double_clicked() {
                        actions.push(FileAction::Open(job.clone()));
                    }
                    if response.secondary_clicked() && !self.selected.contains(&job.id) {
                        self.selected.clear();
                        self.selected.insert(job.id.clone());
                    }
                    response.context_menu(|ui| {
                        for (kind, cs, en) in [
                            (0, "Informace o souboru", "File information"),
                            (1, "Otevřít soubor", "Open file"),
                            (2, "Otevřít umístění", "Open location"),
                            (3, "Kopírovat soubor", "Copy file"),
                            (4, "Kopírovat do…", "Copy to…"),
                            (5, "Přesunout do…", "Move to…"),
                            (6, "Kopírovat cestu", "Copy path"),
                            (7, "Otevřít zdroj", "Open source"),
                        ] {
                            if ui
                                .add_enabled(
                                    kind == 0 || kind == 7 || job.state == JobState::Completed,
                                    egui::Button::new(tr(&lang, cs, en)),
                                )
                                .clicked()
                            {
                                actions.push(match kind {
                                    0 => FileAction::Info(job.clone()),
                                    1 => FileAction::Open(job.clone()),
                                    2 => FileAction::Folder(job.clone()),
                                    3 => FileAction::CopyFile(job.clone()),
                                    4 => FileAction::Transfer(job.clone(), false),
                                    5 => FileAction::Transfer(job.clone(), true),
                                    6 => FileAction::CopyPath(job.clone()),
                                    _ => FileAction::Source(job.clone()),
                                });
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui
                            .button(tr(
                                &lang,
                                "Odebrat z tohoto seznamu",
                                "Remove from this list",
                            ))
                            .clicked()
                        {
                            actions.push(FileAction::Remove(job.clone(), library));
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                job.state == JobState::Completed,
                                egui::Button::new(
                                    RichText::new(tr(
                                        &lang,
                                        "Odstranit soubor z disku…",
                                        "Delete file from disk…",
                                    ))
                                    .color(RED),
                                ),
                            )
                            .clicked()
                        {
                            actions.push(FileAction::Recycle(job.clone()));
                            ui.close();
                        }
                    });
                });
            });
        ui.separator();
        ui.horizontal(|ui| {
            style::caption(
                ui,
                &format!(
                    "{} / {} · {}: {}",
                    jobs.len(),
                    all.len(),
                    tr(&lang, "Vybráno", "Selected"),
                    self.selected.len()
                ),
            );
            if let Some(j) = self
                .selected
                .iter()
                .next()
                .and_then(|id| self.state.jobs.iter().find(|j| &j.id == id))
            {
                ui.add(
                    egui::Label::new(
                        RichText::new(model::message(&lang, &j.message))
                            .color(MUTED)
                            .size(12.),
                    )
                    .truncate(),
                );
            }
        });
        ui.horizontal_wrapped(|ui| {
            let selected: Vec<_> = all
                .iter()
                .filter(|j| self.selected.contains(&j.id))
                .cloned()
                .collect();
            if !library {
                for (to, cs, en) in [
                    (JobState::Queued, "Obnovit", "Resume"),
                    (JobState::Paused, "Pozastavit", "Pause"),
                    (JobState::Cancelled, "Zrušit úlohu", "Cancel"),
                ] {
                    if ui
                        .add_enabled(!selected.is_empty(), egui::Button::new(tr(&lang, cs, en)))
                        .clicked()
                    {
                        for j in &selected {
                            self.engine.change_state(&j.id, to);
                        }
                    }
                }
                if ui
                    .add_enabled(
                        !selected.is_empty(),
                        egui::Button::new(tr(&lang, "Na začátek", "Prioritize")),
                    )
                    .clicked()
                {
                    for j in selected.iter().rev() {
                        self.engine.prioritize(&j.id);
                    }
                }
            }
            if ui
                .add_enabled(
                    selected.len() == 1,
                    egui::Button::new(tr(&lang, "Otevřít", "Open")),
                )
                .clicked()
            {
                actions.push(FileAction::Open(selected[0].clone()));
            }
            if ui
                .add_enabled(
                    selected.len() == 1,
                    egui::Button::new(tr(&lang, "Umístění", "Location")),
                )
                .clicked()
            {
                actions.push(FileAction::Folder(selected[0].clone()));
            }
            if ui
                .add_enabled(
                    !selected.is_empty(),
                    egui::Button::new(tr(&lang, "Odebrat vybrané", "Remove selected")),
                )
                .clicked()
            {
                self.confirm = Some(Confirm::Clear(self.selected.clone(), library));
            }
            if ui
                .add_enabled(
                    !all.is_empty(),
                    egui::Button::new(
                        RichText::new(tr(
                            &lang,
                            if library {
                                "Vyprázdnit knihovnu…"
                            } else {
                                "Vyčistit seznam…"
                            },
                            if library {
                                "Clear library…"
                            } else {
                                "Clear list…"
                            },
                        ))
                        .color(RED),
                    ),
                )
                .clicked()
            {
                self.confirm = Some(Confirm::Clear(
                    all.iter().map(|j| j.id.clone()).collect(),
                    library,
                ));
            }
        });
        for action in actions {
            self.file_action(ui.ctx(), action);
        }
    }
    fn header(&mut self, ui: &mut egui::Ui, id: &str, all: &[Job]) {
        let lang = self.settings.language.clone();
        let (label, asc, desc) = match id {
            "Name" => (tr(&lang, "NÁZEV", "NAME"), Sort::NameAsc, Sort::NameDesc),
            "Source" => (
                tr(&lang, "ZDROJ", "SOURCE"),
                Sort::SourceAsc,
                Sort::SourceDesc,
            ),
            "State" => (tr(&lang, "STAV", "STATUS"), Sort::StateAsc, Sort::StateDesc),
            "Progress" => (
                tr(&lang, "PRŮBĚH", "PROGRESS"),
                Sort::ProgressAsc,
                Sort::ProgressDesc,
            ),
            _ => (
                tr(&lang, "RYCHLOST", "SPEED"),
                Sort::SpeedAsc,
                Sort::SpeedDesc,
            ),
        };
        let active = match id {
            "Name" => !self.filter.query.is_empty(),
            "Source" => !self.filter.source.is_empty(),
            "State" => self.filter.state.is_some(),
            _ => false,
        };
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, 2., Color32::from_rgb(34, 57, 68));
        }
        let arrow = if self.sort == asc {
            "↑"
        } else if self.sort == desc {
            "↓"
        } else {
            "▾"
        };
        ui.painter().text(
            rect.left_center() + egui::vec2(9., 0.),
            egui::Align2::LEFT_CENTER,
            format!("{label}  {arrow}"),
            egui::FontId::proportional(12.),
            if active { LIME } else { MUTED },
        );
        egui::Popup::menu(&response).show(|ui| {
            for (sort, cs, en) in [
                (asc, "Seřadit vzestupně", "Sort ascending"),
                (desc, "Seřadit sestupně", "Sort descending"),
            ] {
                if ui
                    .selectable_label(self.sort == sort, tr(&lang, cs, en))
                    .clicked()
                {
                    self.sort = sort;
                    ui.close();
                }
            }
            if id == "Name" {
                for (sort, cs, en) in [
                    (Sort::Newest, "Nejnovější nahoře", "Newest first"),
                    (Sort::Oldest, "Nejstarší nahoře", "Oldest first"),
                ] {
                    if ui
                        .selectable_label(self.sort == sort, tr(&lang, cs, en))
                        .clicked()
                    {
                        self.sort = sort;
                        ui.close();
                    }
                }
            }
            if matches!(id, "Name" | "Source" | "State") {
                ui.separator();
            }
            match id {
                "Name" => {
                    ui.label(tr(&lang, "Název obsahuje", "Name contains"));
                    ui.add(style::text_field(&mut self.filter.query));
                    if ui
                        .button(tr(&lang, "Zrušit filtr", "Clear filter"))
                        .clicked()
                    {
                        self.filter.query.clear();
                        ui.close();
                    }
                }
                "Source" => {
                    if ui
                        .selectable_label(
                            self.filter.source.is_empty(),
                            tr(&lang, "Všechny zdroje", "All sources"),
                        )
                        .clicked()
                    {
                        self.filter.source.clear();
                        ui.close();
                    }
                    for source in model::SOURCES {
                        if all.iter().any(|j| j.source == source)
                            && ui
                                .selectable_label(
                                    self.filter.source == source,
                                    source_label(source, &lang),
                                )
                                .clicked()
                        {
                            self.filter.source = source.into();
                            ui.close();
                        }
                    }
                }
                "State" => {
                    if ui
                        .selectable_label(
                            self.filter.state.is_none(),
                            tr(&lang, "Všechny položky", "All items"),
                        )
                        .clicked()
                    {
                        self.filter.state = None;
                        ui.close();
                    }
                    for state in [
                        JobState::Queued,
                        JobState::Connecting,
                        JobState::Downloading,
                        JobState::Processing,
                        JobState::Paused,
                        JobState::Completed,
                        JobState::Failed,
                        JobState::Cancelled,
                    ] {
                        if ui
                            .selectable_label(self.filter.state == Some(state), state.label(&lang))
                            .clicked()
                        {
                            self.filter.state = Some(state);
                            ui.close();
                        }
                    }
                }
                _ => {}
            }
        });
    }
    fn file_action(&mut self, ctx: &egui::Context, action: FileAction) {
        match action {
            FileAction::Info(j) => self.info = Some(j),
            FileAction::Open(j) => {
                let r = if Path::new(&j.file_path).is_file() {
                    open::that(&j.file_path).map_err(Into::into)
                } else {
                    Err(anyhow::anyhow!(
                        self.t("Soubor nebyl nalezen", "File not found")
                    ))
                };
                self.result(r.map(|_| ""));
            }
            FileAction::Folder(j) => {
                self.result(
                    platform::open_location(Path::new(if j.file_path.is_empty() {
                        &j.directory
                    } else {
                        &j.file_path
                    }))
                    .map(|_| ""),
                );
            }
            FileAction::CopyFile(j) => {
                self.result(platform::copy_file(Path::new(&j.file_path)).map(|_| {
                    self.t("Soubor ve schránce", "File copied to clipboard")
                        .to_owned()
                }))
            }
            FileAction::CopyPath(j) => {
                ctx.copy_text(j.file_path);
                self.notice = self.t("Cesta ve schránce", "Path copied").into();
            }
            FileAction::Source(j) => {
                self.result(
                    model::parse_url(&j.url)
                        .and_then(|u| open::that(u.as_str()).map_err(Into::into))
                        .map(|_| ""),
                );
            }
            FileAction::Transfer(j, moving) => {
                if let Some(folder) = rfd::FileDialog::new()
                    .set_directory(&j.directory)
                    .pick_folder()
                {
                    let engine = self.engine.clone();
                    self.spawn(ctx, move || {
                        engine
                            .transfer(&j.id, &folder, moving)
                            .map(|p| p.display().to_string())
                    });
                }
            }
            FileAction::Recycle(j) => self.confirm = Some(Confirm::Recycle(j.id)),
            FileAction::Remove(j, library) => {
                self.confirm = Some(Confirm::Clear(HashSet::from([j.id]), library))
            }
        }
    }
}

fn state_color(state: JobState) -> Color32 {
    match state {
        JobState::Completed => LIME,
        JobState::Failed => RED,
        JobState::Downloading | JobState::Processing => BLUE,
        _ => MUTED,
    }
}
fn source_label<'a>(source: &'a str, lang: &str) -> &'a str {
    if source == "Web" {
        tr(lang, "Ostatní weby", "Other sites")
    } else {
        source
    }
}
fn quality_label(q: i32, lang: &str) -> String {
    if q < 0 {
        tr(lang, "Podle výchozího nastavení", "Use default").into()
    } else if q == 0 {
        tr(lang, "Nejvyšší dostupná", "Best available").into()
    } else {
        format!("{q}p")
    }
}
fn quality_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    q: &mut i32,
    inherit: bool,
    lang: &str,
) {
    egui::ComboBox::from_id_salt(id)
        .width(170.)
        .selected_text(quality_label(*q, lang))
        .show_ui(ui, |ui| {
            if inherit {
                ui.selectable_value(q, -1, quality_label(-1, lang));
            }
            for value in model::QUALITIES {
                ui.selectable_value(q, value, quality_label(value, lang));
            }
        });
}
fn format_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    f: &mut String,
    inherit: bool,
    lang: &str,
) {
    egui::ComboBox::from_id_salt(id)
        .width(120.)
        .selected_text(if f.is_empty() {
            tr(lang, "Výchozí formát", "Use default")
        } else {
            f.as_str()
        })
        .show_ui(ui, |ui| {
            if inherit {
                ui.selectable_value(f, String::new(), tr(lang, "Výchozí formát", "Use default"));
            }
            for value in ["MP4", "MKV", "MP3"] {
                ui.selectable_value(f, value.into(), value);
            }
        });
}
fn sort_jobs(jobs: &mut [Job], sort: Sort) {
    jobs.sort_by(|a, b| {
        let text = |s: &str| s.to_lowercase();
        match sort {
            Sort::Newest => b.created.cmp(&a.created),
            Sort::Oldest => a.created.cmp(&b.created),
            Sort::NameAsc => text(&a.title).cmp(&text(&b.title)),
            Sort::NameDesc => text(&b.title).cmp(&text(&a.title)),
            Sort::SourceAsc => a.source.cmp(&b.source),
            Sort::SourceDesc => b.source.cmp(&a.source),
            Sort::StateAsc => (a.state as u8).cmp(&(b.state as u8)),
            Sort::StateDesc => (b.state as u8).cmp(&(a.state as u8)),
            Sort::ProgressAsc => a.percent.total_cmp(&b.percent),
            Sort::ProgressDesc => b.percent.total_cmp(&a.percent),
            Sort::SpeedAsc => a.speed.total_cmp(&b.speed),
            Sort::SpeedDesc => b.speed.total_cmp(&a.speed),
        }
        .then_with(|| b.created.cmp(&a.created))
        .then_with(|| a.id.cmp(&b.id))
    });
}

impl App {
    fn begin_setup(&mut self) {
        self.invalidate_cookie_request();
        self.setup_saved = Some(self.settings.clone());
        self.setup = Some(0);
        self.notice.clear();
    }
    fn invalidate_cookie_request(&mut self) {
        self.cookie_epoch = self.cookie_epoch.wrapping_add(1);
        self.cookie_cancel.store(true, Ordering::Relaxed);
        self.cookie_results.clear();
    }
    fn cancel_setup(&mut self) {
        self.invalidate_cookie_request();
        if let Some(saved) = self.setup_saved.take() {
            self.settings = saved;
        }
        self.setup = None;
    }

    fn export_installer(&mut self, ctx: &egui::Context) {
        let profile = match crate::installer::Profile::from_settings(
            &self.settings,
            self.builder_folder.clone(),
            self.install_location.clone(),
        ) {
            Ok(p) => p,
            Err(e) => {
                self.result(Err::<String, _>(e));
                return;
            }
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Windows application", &["exe"])
            .set_file_name("WorkFlow Media Setup.exe")
            .save_file()
        else {
            return;
        };
        let source = self.exe.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy += 1;
        std::thread::spawn(move || {
            let result = crate::installer::write_package(
                &source,
                &path,
                &crate::installer::Package::Installer(profile),
            )
            .map(|_| path);
            let _ = tx.send(Event::Exported(result));
            ctx.request_repaint();
        });
    }

    fn installer_summary(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        style::title(
            ui,
            tr(&lang, "Co příjemce dostane", "What the recipient gets"),
        );
        ui.label(tr(&lang, "Jeden EXE: stažení aplikace z GitHubu, ověření podpisu, instalace a hotové nastavení.", "One EXE: download from GitHub, signature verification, installation and prepared settings."));
        ui.add_space(12.);
        egui::Grid::new("installer-summary")
            .spacing([24., 12.])
            .show(ui, |ui| {
                ui.label(tr(&lang, "Stahování", "Downloads"));
                ui.label(format!(
                    "{} / {}",
                    tr(&lang, "Stažené u příjemce", "Recipient's Downloads"),
                    self.builder_folder
                ));
                ui.end_row();
                ui.label(tr(&lang, "Souběžné úlohy", "Parallel downloads"));
                ui.label(self.settings.parallel.to_string());
                ui.end_row();
                ui.label(tr(&lang, "Kvalita a formát", "Quality and format"));
                ui.label(format!(
                    "{} · {}",
                    quality_label(self.settings.quality, &lang),
                    self.settings.format
                ));
                ui.end_row();
                ui.label(tr(&lang, "Spuštění po přihlášení", "Start at sign-in"));
                ui.label(tr(
                    &lang,
                    if self.settings.start_with_windows {
                        "Ano"
                    } else {
                        "Ne"
                    },
                    if self.settings.start_with_windows {
                        "Yes"
                    } else {
                        "No"
                    },
                ));
                ui.end_row();
                ui.label(tr(&lang, "Schránka", "Clipboard"));
                ui.label(match self.settings.clipboard_mode {
                    ClipboardMode::Off => tr(&lang, "Vypnuto", "Off").into(),
                    ClipboardMode::All => tr(
                        &lang,
                        "Všechny podporované služby",
                        "All supported services",
                    )
                    .into(),
                    ClipboardMode::Selected => self.settings.clipboard_sources.join(", "),
                });
                ui.end_row();
                ui.label(tr(&lang, "Cookies z prohlížeče", "Browser cookies"));
                ui.label(if self.settings.cookies_mode == model::CookieMode::Single {
                    self.settings
                        .cookies_browser
                        .map(|b| b.label())
                        .unwrap_or(tr(&lang, "Vypnuto", "Off"))
                } else {
                    tr(
                        &lang,
                        "Všechny profily vybraných prohlížečů",
                        "All profiles in the selected browsers",
                    )
                });
                ui.end_row();
            });
        ui.add_space(14.);
        self.installer_locations(ui);
        style::caption(
            ui,
            tr(
                &lang,
                "Příjemce potvrdí správce. Instalace a automatické spouštění patří jeho uživatelskému účtu. Hesla, cookies a soukromé soubory se nepřenášejí.",
                "The recipient confirms the administrator prompt. Installation and autostart belong to their user account. Passwords, cookies and private files are not included.",
            ),
        );
        if let Some(path) = &self.created_installer {
            ui.add_space(16.);
            ui.colored_label(
                LIME,
                tr(&lang, "Instalační soubor vytvořen", "Installer created"),
            );
            ui.add(egui::Label::new(path.display().to_string()).wrap());
            if ui
                .button(tr(&lang, "Zobrazit soubor", "Show file"))
                .clicked()
            {
                let _ = platform::open_location(path);
            }
        }
    }

    fn installer_locations(&mut self, ui: &mut egui::Ui) {
        use crate::installer::UserFolder;
        let lang = self.settings.language.clone();
        ui.separator();
        style::title(ui, tr(&lang, "Umístění u příjemce", "Recipient locations"));
        ui.label(tr(
            &lang,
            "Nainstalovat aplikaci do",
            "Install the application in",
        ));
        egui::ComboBox::from_id_salt("install-folder")
            .selected_text(self.install_location.folder.label(&lang))
            .show_ui(ui, |ui| {
                for folder in [
                    UserFolder::Documents,
                    UserFolder::Desktop,
                    UserFolder::Downloads,
                    UserFolder::Programs,
                ] {
                    ui.selectable_value(
                        &mut self.install_location.folder,
                        folder,
                        folder.label(&lang),
                    );
                }
            });
        ui.label(tr(&lang, "Podsložka aplikace", "Application subfolder"));
        style::wrapping_field(
            ui,
            &mut self.install_location.subfolder,
            ui.available_width(),
            "WorkFlow Media",
        );
        ui.label(tr(&lang, "Umístit zástupce do", "Place the shortcut in"));
        egui::ComboBox::from_id_salt("shortcut-folder")
            .selected_text(
                self.install_location
                    .shortcut
                    .map(|f| f.label(&lang))
                    .unwrap_or(tr(&lang, "Bez zástupce", "No shortcut")),
            )
            .show_ui(ui, |ui| {
                for folder in [
                    UserFolder::Desktop,
                    UserFolder::Documents,
                    UserFolder::Downloads,
                    UserFolder::StartMenu,
                ] {
                    ui.selectable_value(
                        &mut self.install_location.shortcut,
                        Some(folder),
                        folder.label(&lang),
                    );
                }
                ui.selectable_value(
                    &mut self.install_location.shortcut,
                    None,
                    tr(&lang, "Bez zástupce", "No shortcut"),
                );
            });
        ui.add(egui::Label::new(tr(&lang,
            "Složky se zjistí až na počítači příjemce. Fungují i při jiném jménu uživatele nebo přesměrování do OneDrivu.",
            "Folders are resolved on the recipient's computer, including a different username or OneDrive redirection.")).wrap());
        ui.add_space(8.);
    }

    fn general_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(tr(&lang, "Jazyk aplikace", "Application language"));
                egui::ComboBox::from_id_salt("language")
                    .selected_text(if lang == "cs" { "Čeština" } else { "English" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.settings.language, "cs".into(), "Čeština");
                        ui.selectable_value(&mut self.settings.language, "en".into(), "English");
                    });
            });
            ui.add_space(20.);
            ui.vertical(|ui| {
                ui.label(tr(&lang, "Souběžná stahování", "Parallel downloads"));
                egui::ComboBox::from_id_salt("parallel")
                    .height(320.)
                    .selected_text(self.settings.parallel.to_string())
                    .show_ui(ui, |ui| {
                        for n in 1..=model::MAX_PARALLEL {
                            ui.selectable_value(&mut self.settings.parallel, n, n.to_string());
                        }
                    });
            });
        });
        ui.add_space(8.);
        ui.label(if self.builder {
            tr(
                &lang,
                "Podsložka ve Stažených u příjemce",
                "Subfolder in recipient's Downloads",
            )
        } else {
            tr(&lang, "Výchozí složka", "Default folder")
        });
        if self.builder {
            style::wrapping_field(ui, &mut self.builder_folder, ui.available_width(), "");
        } else {
            ui.horizontal_top(|ui| {
                let width = ui.available_width() - 40. - ui.spacing().item_spacing.x;
                style::wrapping_field(ui, &mut self.settings.download_directory, width, "");
                if ui.add_sized([40., 34.], egui::Button::new("…")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_directory(&self.settings.download_directory)
                        .pick_folder()
                    {
                        self.settings.download_directory = path.to_string_lossy().into();
                    }
                }
            });
        }
        ui.add_space(10.);
        style::checkbox(
            ui,
            &mut self.settings.start_with_windows,
            tr(
                &lang,
                "Spouštět po přihlášení k počítači na pozadí",
                "Start in the background when I sign in",
            ),
        );
        style::checkbox(
            ui,
            &mut self.settings.close_to_tray,
            tr(
                &lang,
                "Křížek skryje okno k hodinám; minimalizace ponechá okno na liště",
                "Close button hides to tray; minimize keeps the taskbar button",
            ),
        );
        style::checkbox(
            ui,
            &mut self.settings.start_on_add,
            tr(
                &lang,
                "Nové ručně přidané úlohy rovnou spustit",
                "Start manually added downloads immediately",
            ),
        );
    }
    fn clipboard_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        style::title(
            ui,
            tr(
                &lang,
                "Co se má stát po zkopírování odkazu?",
                "What happens when you copy a link?",
            ),
        );
        let label = match self.settings.clipboard_mode {
            ClipboardMode::Off => tr(
                &lang,
                "Vypnuto · odkazy přidávám ručně",
                "Off · I add links manually",
            ),
            ClipboardMode::Selected => {
                tr(&lang, "Jen z vybraných služeb", "Selected services only")
            }
            ClipboardMode::All => tr(&lang, "Ze všech podporovaných webů", "All supported sites"),
        };
        egui::ComboBox::from_id_salt("clipboard-mode")
            .width(ui.available_width())
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (mode, cs, en) in [
                    (
                        ClipboardMode::Off,
                        "Vypnuto · odkazy přidávám ručně",
                        "Off · I add links manually",
                    ),
                    (
                        ClipboardMode::Selected,
                        "Jen z vybraných služeb",
                        "Selected services only",
                    ),
                    (
                        ClipboardMode::All,
                        "Ze všech podporovaných webů",
                        "All supported sites",
                    ),
                ] {
                    ui.selectable_value(&mut self.settings.clipboard_mode, mode, tr(&lang, cs, en));
                }
            });
        ui.add_space(16.);
        ui.label(tr(&lang, "Povolené služby", "Enabled services"));
        ui.add_enabled_ui(
            self.settings.clipboard_mode == ClipboardMode::Selected,
            |ui| {
                egui::Grid::new("clipboard-services")
                    .num_columns(2)
                    .min_col_width(250.)
                    .spacing([40., 14.])
                    .show(ui, |ui| {
                        for (i, source) in model::SOURCES.iter().enumerate() {
                            let mut checked =
                                self.settings.clipboard_sources.iter().any(|s| s == source);
                            if style::checkbox(ui, &mut checked, source_label(source, &lang))
                                .changed()
                            {
                                self.settings.clipboard_sources.retain(|s| s != source);
                                if checked {
                                    self.settings.clipboard_sources.push(source.to_string());
                                }
                            }
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
            },
        );
        ui.add_space(14.);
        style::caption(
            ui,
            tr(
                &lang,
                "Stačí zkopírovat samotný odkaz nebo více odkazů na samostatných řádcích. Použije se složka a předvolba služby. Pozastavená fronta zůstane pozastavená.",
                "Copy a standalone link, or several links on separate lines. Downloads use the default folder and service preset. A paused queue stays paused.",
            ),
        );
    }
    fn quality_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        egui::Grid::new("quality-settings")
            .spacing([24., 16.])
            .min_col_width(120.)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(tr(&lang, "Výchozí nastavení", "Default settings")).strong(),
                );
                ui.label(tr(
                    &lang,
                    "Maximální rozlišení videa",
                    "Maximum video resolution",
                ));
                ui.label(tr(&lang, "Formát", "Format"));
                ui.end_row();
                ui.label(tr(&lang, "Všechny služby", "All services"));
                quality_combo(
                    ui,
                    "default-quality",
                    &mut self.settings.quality,
                    false,
                    &lang,
                );
                format_combo(
                    ui,
                    "default-format",
                    &mut self.settings.format,
                    false,
                    &lang,
                );
                ui.end_row();
                for source in model::SOURCES {
                    ui.label(source_label(source, &lang));
                    if source == "FastShare" {
                        ui.colored_label(MUTED, tr(&lang, "Původní soubor", "Original file"));
                        ui.colored_label(MUTED, tr(&lang, "Originál", "Original"));
                    } else {
                        let p = self
                            .settings
                            .service_preferences
                            .entry(source.into())
                            .or_default();
                        quality_combo(ui, format!("quality-{source}"), &mut p.quality, true, &lang);
                        format_combo(ui, format!("format-{source}"), &mut p.format, true, &lang);
                    }
                    ui.end_row();
                }
            });
        ui.add_space(16.);
        style::caption(
            ui,
            tr(
                &lang,
                "Nejvyšší dostupná = nejlepší kvalita, kterou odkaz nabízí. Pokud rozlišení není k dispozici, použije se nižší. MP3 ukládá pouze zvuk v nejlepší dostupné kvalitě.",
                "Best available uses the highest quality offered by the link. If the selected resolution is unavailable, a lower one is used. MP3 saves audio only, at the best available quality.",
            ),
        );
    }
    fn update_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        style::checkbox(
            ui,
            &mut self.settings.automatic_updates,
            tr(
                &lang,
                "Automaticky aktualizovat aplikaci z GitHubu",
                "Automatically update the application from GitHub",
            ),
        );
        style::caption(
            ui,
            tr(
                &lang,
                "Kontrola jednou denně. Podepsaná aktualizace se nainstaluje a aplikace restartuje až po dokončení fronty. Vaše data a předvolby zůstanou zachované.",
                "Checks once a day. Signed updates install and restart the application once the queue is idle. Your downloads, library and preferences are preserved.",
            ),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("WorkFlow Media {}", updater::VERSION));
            ui.hyperlink_to(
                "GitHub Releases",
                format!("https://github.com/{}/releases", updater::REPOSITORY),
            );
            if ui
                .add_enabled(
                    self.busy == 0,
                    egui::Button::new(tr(&lang, "Zkontrolovat aktualizace", "Check for updates")),
                )
                .clicked()
            {
                self.check_update(ui.ctx(), false);
            }
            if let Some(m) = &self.update {
                ui.colored_label(LIME, format!("v{}", m.version));
                if self.update_file.is_none()
                    && ui
                        .add_enabled(
                            self.busy == 0,
                            egui::Button::new(tr(&lang, "Stáhnout aktualizaci", "Download update")),
                        )
                        .clicked()
                {
                    self.download_update(ui.ctx());
                }
            }
            if self.update_file.is_some()
                && ui
                    .button(tr(
                        &lang,
                        "Nainstalovat a restartovat",
                        "Install and restart",
                    ))
                    .clicked()
            {
                self.confirm = Some(Confirm::Quit);
            }
        });
    }
    fn account_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(9.), egui::Sense::hover());
            ui.painter().circle_filled(
                dot.center(),
                3.5,
                if self.engine.fast.logged_in() {
                    GREEN
                } else {
                    MUTED
                },
            );
            ui.label(RichText::new(self.engine.fast.account(&lang)).color(MUTED));
        });
        ui.add_space(4.);
        if self.engine.fast.logged_in() {
            if self.engine.fast.password_saved()
                && ui
                    .add_enabled(
                        self.busy == 0,
                        egui::Button::new(tr(&lang, "Obnovit přihlášení", "Refresh sign-in")),
                    )
                    .clicked()
            {
                let fast = self.engine.fast.clone();
                let text = tr(&lang, "Přihlášení obnoveno", "Sign-in refreshed").to_owned();
                self.spawn(ui.ctx(), move || fast.renew_login().map(|_| text));
            }
            if ui
                .button(tr(&lang, "Odhlásit a zapomenout", "Sign out and forget"))
                .clicked()
            {
                self.result(self.engine.fast.logout().map(|_| ""));
                self.login.clear();
                self.password.clear();
            }
        } else {
            let mut submit = false;
            ui.horizontal(|ui| {
                let field_width = (ui.available_width() - 156.) / 2.;
                ui.vertical(|ui| {
                    style::caption(ui, tr(&lang, "Uživatelské jméno", "Username"));
                    ui.add_sized(
                        [field_width, 38.],
                        style::text_field(&mut self.login)
                            .margin(egui::vec2(10., 9.))
                            .hint_text(tr(
                                &lang,
                                "Vaše jméno na FastShare",
                                "Your FastShare username",
                            )),
                    );
                });
                ui.vertical(|ui| {
                    style::caption(ui, tr(&lang, "Heslo", "Password"));
                    let response = ui.add_sized(
                        [field_width, 38.],
                        style::text_field(&mut self.password)
                            .margin(egui::vec2(10., 9.))
                            .password(true)
                            .hint_text(tr(&lang, "Vaše heslo", "Your password")),
                    );
                    submit = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                ui.vertical(|ui| {
                    style::caption(ui, " ");
                    submit |= ui
                        .add_enabled_ui(self.busy == 0, |ui| {
                            style::primary_sized(
                                ui,
                                tr(&lang, "Přihlásit", "Sign in"),
                                Vec2::new(136., 38.),
                            )
                        })
                        .inner
                        .clicked();
                });
            });
            if submit && self.busy == 0 {
                let fast = self.engine.fast.clone();
                let username = self.login.clone();
                let password = std::mem::take(&mut self.password);
                let success = tr(&lang, "Přihlášeno", "Signed in").to_owned();
                self.spawn(ui.ctx(), move || {
                    fast.login(&username, &password).map(|_| success)
                });
            }
        }
        style::caption(
            ui,
            tr(
                &lang,
                "Přihlášení si bezpečně zapamatujeme. Heslo chrání systémové šifrování.",
                "Your sign-in is remembered securely, protected by system encryption.",
            ),
        );
    }
    fn load_cookie_profiles(&mut self, ctx: &egui::Context, refresh: bool) {
        if self.cookies_busy {
            return;
        }
        if let Some(previous) = self.cookie_task.take() {
            let _ = previous.join();
        }
        self.cookie_epoch = self.cookie_epoch.wrapping_add(1);
        self.cookie_cancel = Arc::new(AtomicBool::new(false));
        self.cookies_busy = true;
        self.busy += 1;
        let settings = self.settings.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let cancel = self.cookie_cancel.clone();
        let epoch = self.cookie_epoch;
        self.cookie_task = Some(std::thread::spawn(move || {
            let result = if refresh {
                crate::cookies::refresh(&settings, &cancel)
            } else {
                Ok(crate::cookies::RefreshReport {
                    profiles: crate::cookies::discover(
                        if settings.cookies_mode == model::CookieMode::All {
                            &settings.cookies_browsers
                        } else {
                            &model::Browser::ALL
                        },
                    ),
                    results: Vec::new(),
                })
            };
            let _ = tx.send(Event::Cookies(result, refresh, epoch));
            ctx.request_repaint();
        }));
    }
    fn cookies_settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        style::title(
            ui,
            tr(
                &lang,
                "Cookies a profily prohlížečů",
                "Cookies and browser profiles",
            ),
        );
        ui.add_enabled_ui(!self.cookies_busy, |ui| self.cookie_controls(ui));
        if self.cookies_busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr(
                    &lang,
                    "Načítám profily na pozadí…",
                    "Loading profiles in the background…",
                ));
            });
        }
    }
    fn cookie_controls(&mut self, ui: &mut egui::Ui) {
        use model::CookieMode;
        let lang = self.settings.language.clone();
        style::caption(
            ui,
            tr(
                &lang,
                "Každý odkaz se nejdříve zkusí bez cookies. Účty se použijí až při odmítnutí přístupu nebo požadavku na přihlášení.",
                "Every link is tried without cookies first. Accounts are used only after an access denial or sign-in requirement.",
            ),
        );
        let mut enabled = self.settings.uses_browser_cookies();
        if style::checkbox(
            ui,
            &mut enabled,
            tr(
                &lang,
                "Používat cookies z prohlížečů",
                "Use browser cookies",
            ),
        )
        .changed()
        {
            self.settings.cookies_browser = enabled.then_some(model::Browser::Chrome);
            if !enabled {
                self.settings.cookies_mode = CookieMode::Single;
            }
        }
        if enabled {
            ui.add_space(8.);
            let mode_label = |mode| match mode {
                CookieMode::Single => tr(&lang, "Jeden výchozí profil", "One default profile"),
                CookieMode::Selected => tr(
                    &lang,
                    "Vybrané profily postupně",
                    "Try selected profiles in order",
                ),
                CookieMode::All => tr(
                    &lang,
                    "Vytáhnout všechny cookies",
                    "Load all browser cookies",
                ),
            };
            egui::ComboBox::from_id_salt("cookies_mode")
                .width(300.0_f32.min(ui.available_width()))
                .selected_text(mode_label(self.settings.cookies_mode))
                .show_ui(ui, |ui| {
                    for mode in [CookieMode::Single, CookieMode::Selected, CookieMode::All] {
                        if self.builder && mode == CookieMode::Selected {
                            continue;
                        }
                        if ui
                            .selectable_value(
                                &mut self.settings.cookies_mode,
                                mode,
                                mode_label(mode),
                            )
                            .changed()
                            && mode == CookieMode::Single
                            && self.settings.cookies_browser.is_none()
                        {
                            self.settings.cookies_browser = Some(model::Browser::Chrome);
                        }
                    }
                });
            if self.settings.cookies_mode == CookieMode::Single {
                let mut browser = self
                    .settings
                    .cookies_browser
                    .unwrap_or(model::Browser::Chrome);
                egui::ComboBox::from_id_salt("cookies_browser")
                    .width(240.0_f32.min(ui.available_width()))
                    .selected_text(browser.label())
                    .show_ui(ui, |ui| {
                        for choice in model::Browser::ALL {
                            ui.selectable_value(&mut browser, choice, choice.label());
                        }
                    });
                self.settings.cookies_browser = Some(browser);
            } else if self.settings.cookies_mode == CookieMode::All {
                ui.label(tr(
                    &lang,
                    "Prohledat tyto prohlížeče:",
                    "Search these browsers:",
                ));
                let columns = if ui.available_width() >= 620. {
                    3
                } else if ui.available_width() >= 340. {
                    2
                } else {
                    1
                };
                ui.columns(columns, |uis| {
                    for (i, browser) in model::Browser::ALL.into_iter().enumerate() {
                        let ui = &mut uis[i % columns];
                        let mut selected = self.settings.cookies_browsers.contains(&browser);
                        if style::checkbox(ui, &mut selected, browser.label()).changed() {
                            if selected {
                                self.settings.cookies_browsers.push(browser);
                            } else {
                                self.settings.cookies_browsers.retain(|b| *b != browser);
                            }
                        }
                    }
                });
            }
            ui.add_space(8.);
            style::checkbox(
                ui,
                &mut self.settings.cookies_auto_refresh,
                tr(
                    &lang,
                    "Automaticky aktualizovat cookies profilů",
                    "Automatically refresh profile cookies",
                ),
            );
            style::caption(
                ui,
                tr(
                    &lang,
                    "Cookies obnoví pouze při potřebě přihlášení, nejvýše jednou za minutu pro každý profil. Při vypnutí nebo nedostupném prohlížeči použije poslední kopii. První načtení je automatické; nové přihlášení proveďte v prohlížeči.",
                    "Refreshes cookies only when sign-in is needed, at most once per minute per profile. When disabled or the browser is unavailable, the last snapshot is used. The first load is automatic; sign in again in the browser when needed.",
                ),
            );
            if self.builder {
                ui.add_space(10.);
                style::caption(
                    ui,
                    tr(
                        &lang,
                        "Instalátor přenese pouze režim, prohlížeče a automatické obnovování. Všechny profily vyhledá až u příjemce. Vaše cookies, názvy účtů ani cesty neobsahuje.",
                        "The installer includes only the mode, browsers and refresh preference. Profiles are discovered on the recipient’s computer. Your cookies, account names and paths are excluded.",
                    ),
                );
            } else {
                ui.add_space(10.);
                ui.horizontal_wrapped(|ui| {
                    if self.settings.cookies_mode != CookieMode::Single
                        && ui
                            .button(tr(&lang, "Vyhledat profily", "Find profiles"))
                            .clicked()
                    {
                        self.load_cookie_profiles(ui.ctx(), false);
                    }
                    if ui
                        .button(if self.settings.cookies_mode == CookieMode::All {
                            tr(
                                &lang,
                                "Vytáhnout všechny cookies",
                                "Load all browser cookies",
                            )
                        } else {
                            tr(&lang, "Obnovit cookies nyní", "Refresh cookies now")
                        })
                        .clicked()
                    {
                        self.load_cookie_profiles(ui.ctx(), true);
                    }
                    if ui
                        .button(tr(
                            &lang,
                            "Zapomenout uložené cookies",
                            "Forget saved cookies",
                        ))
                        .clicked()
                    {
                        let lang = lang.clone();
                        self.spawn(ui.ctx(), move || {
                            crate::cookies::forget()?;
                            Ok(tr(
                                &lang,
                                "Uložené cookies odstraněny. První další použití je načte znovu.",
                                "Saved cookies removed. They will be loaded again on the next use.",
                            )
                            .into())
                        });
                        self.cookie_results.clear();
                    }
                });
                if self.settings.cookies_mode != CookieMode::Single {
                    let selected = self.settings.cookies_mode == CookieMode::Selected;
                    let mut move_row = None;
                    let visible: Vec<_> = self
                        .settings
                        .cookie_profiles
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| {
                            selected || self.settings.cookies_browsers.contains(&p.browser)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    for (position, &i) in visible.iter().enumerate() {
                        let profile = &mut self.settings.cookie_profiles[i];
                        ui.push_id(i, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(position > 0, egui::Button::new("↑").small())
                                    .on_hover_text(tr(&lang, "Zkusit dříve", "Try earlier"))
                                    .clicked()
                                {
                                    move_row = Some((i, visible[position - 1]));
                                }
                                if ui
                                    .add_enabled(
                                        position + 1 < visible.len(),
                                        egui::Button::new("↓").small(),
                                    )
                                    .on_hover_text(tr(&lang, "Zkusit později", "Try later"))
                                    .clicked()
                                {
                                    move_row = Some((i, visible[position + 1]));
                                }
                                ui.label(format!("{}.", position + 1));
                                if selected {
                                    style::checkbox(ui, &mut profile.enabled, &profile.label);
                                } else {
                                    ui.add(egui::Label::new(&profile.label).wrap());
                                }
                            });
                            if let Some((_, result)) = self
                                .cookie_results
                                .iter()
                                .find(|(id, _)| *id == crate::cookies::key(profile))
                            {
                                match result {
                                    Ok(count) => {
                                        ui.colored_label(
                                            GREEN,
                                            format!(
                                                "{}: {count}",
                                                tr(&lang, "Načtené cookies", "Cookies loaded")
                                            ),
                                        );
                                    }
                                    Err(error) => {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(model::localized_error(&lang, error))
                                                    .color(RED),
                                            )
                                            .wrap(),
                                        );
                                    }
                                }
                            }
                        });
                    }
                    if let Some((a, b)) = move_row {
                        if b < self.settings.cookie_profiles.len() {
                            self.settings.cookie_profiles.swap(a, b);
                        }
                    }
                    style::caption(
                        ui,
                        tr(
                            &lang,
                            "Nejdříve vždy bez cookies, potom účty shora dolů. Šipkami změňte prioritu; pořadí zůstane zachované i po obnovení a nové profily se přidají na konec. Po úspěchu se další účty nezkouší. Cookies účtů se neslučují.",
                            "Always without cookies first, then accounts from top to bottom. Use the arrows to change priority; refresh preserves the order and new profiles are appended. Success stops further attempts. Account cookies are never merged.",
                        ),
                    );
                } else if let Some((_, result)) = self.cookie_results.first() {
                    match result {
                        Ok(n) => {
                            ui.colored_label(
                                GREEN,
                                format!("{}: {n}", tr(&lang, "Načtené cookies", "Cookies loaded")),
                            );
                        }
                        Err(e) => {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(model::localized_error(&lang, e)).color(RED),
                                )
                                .wrap(),
                            );
                        }
                    }
                }
            }
            ui.add_space(8.);
            style::caption(
                ui,
                tr(
                    &lang,
                    "Uložené kopie chrání Windows nebo systémová klíčenka Linuxu. Pokud prohlížeč cookies zamyká, zavřete jej. Šifrování Chrome/Brave/Edge může přístup blokovat i nadále; podporovaný profil nebo vlastní cookies.txt zůstává alternativou.",
                    "Snapshots use Windows protection or the Linux keyring. Close the browser if cookies are locked. Chrome/Brave/Edge encryption may still block access; use an accessible profile or your own cookies.txt instead.",
                ),
            );
            return;
        }
        if self.builder {
            return;
        }
        ui.add_space(10.);
        // Reserve both buttons; the field grows vertically for long paths.
        let available = ui.available_width();
        let mut browse = false;
        let mut clear = false;
        let mut controls = |ui: &mut egui::Ui| {
            browse = ui
                .add_sized([40., 34.], egui::Button::new("…"))
                .on_hover_text(tr(&lang, "Vybrat soubor cookies", "Choose cookies file"))
                .clicked();
            clear = ui
                .add_sized([94., 34.], egui::Button::new(tr(&lang, "Vymazat", "Clear")))
                .clicked();
        };
        let hint = tr(
            &lang,
            "Volitelný soubor cookies.txt",
            "Optional cookies.txt file",
        );
        if available < 300. {
            style::wrapping_field(ui, &mut self.settings.cookies_file, available, hint);
            ui.horizontal(&mut controls);
        } else {
            ui.horizontal_top(|ui| {
                let width = available - 134. - 2. * ui.spacing().item_spacing.x;
                style::wrapping_field(ui, &mut self.settings.cookies_file, width, hint);
                controls(ui);
            });
        }
        if browse {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Netscape cookies", &["txt"])
                .pick_file()
            {
                self.settings.cookies_file = path.to_string_lossy().into();
            }
        }
        if clear {
            self.settings.cookies_file.clear();
        }
        ui.add(egui::Label::new(RichText::new(tr(&lang,
            "Pro videa vyžadující přihlášení použijte vlastní export cookies ve formátu Netscape. Soubor se nikam neodesílá; aplikace jej používá pro příslušné weby.",
            "For videos that require sign-in, select your own Netscape-format cookie export. The application uses these cookies for the corresponding sites."))
            .size(12.).color(MUTED)).wrap());
    }
    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        ui.horizontal(|ui| {
            style::title(ui, tr(&lang, "Nastavení", "Settings"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled_ui(!self.cookies_busy, |ui| {
                        style::primary(ui, tr(&lang, "Uložit nastavení", "Save settings"))
                    })
                    .inner
                    .clicked()
                {
                    let r = self
                        .save()
                        .map(|_| tr(&lang, "Nastavení uloženo", "Settings saved").to_string());
                    if r.is_ok() {
                        if let Some(tray) = &mut self.tray {
                            if let Err(e) = tray.set_language(&self.settings.language) {
                                self.notice = e.to_string();
                            }
                        }
                    }
                    self.result(r);
                }
                if ui
                    .button(tr(&lang, "Spustit průvodce znovu", "Run setup again"))
                    .clicked()
                {
                    self.begin_setup();
                }
            });
        });
        ui.add_space(8.);
        egui::ScrollArea::vertical()
            .id_salt("settings-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                style::card().show(ui, |ui| {
                    style::title(
                        ui,
                        tr(&lang, "Aplikace a ukládání", "Application and storage"),
                    );
                    ui.add_space(8.);
                    self.general_settings(ui);
                });
                ui.add_space(12.);
                style::card().show(ui, |ui| self.clipboard_settings(ui));
                ui.add_space(12.);
                style::card().show(ui, |ui| {
                    style::title(ui, tr(&lang, "Kvalita a formáty", "Quality and formats"));
                    ui.add_space(8.);
                    self.quality_settings(ui);
                });
                ui.add_space(12.);
                style::card().show(ui, |ui| {
                    style::title(ui, tr(&lang, "Účet FastShare", "FastShare account"));
                    self.account_settings(ui);
                });
                ui.add_space(12.);
                style::card().show(ui, |ui| self.cookies_settings(ui));
                ui.add_space(12.);
                style::card().show(ui, |ui| {
                    style::title(ui, tr(&lang, "Aktualizace", "Updates"));
                    self.update_settings(ui);
                });
                ui.add_space(12.);
                if cfg!(windows) {
                    style::card().show(ui, |ui| {
                        style::title(
                            ui,
                            tr(
                                &lang,
                                "Instalace pro další počítač",
                                "Installation for another computer",
                            ),
                        );
                        style::caption(
                            ui,
                            tr(
                                &lang,
                                "Připravte jeden instalační EXE s nastavením pro příjemce.",
                                "Prepare a single installer EXE with settings for its recipient.",
                            ),
                        );
                        if ui
                            .button(tr(
                                &lang,
                                "Otevřít generátor instalace",
                                "Open setup builder",
                            ))
                            .clicked()
                        {
                            let result = platform::command(&self.exe)
                                .arg("--setup-builder")
                                .spawn()
                                .map(|_| "")
                                .map_err(Into::into);
                            self.result(result);
                        }
                    });
                    ui.add_space(12.);
                }
                style::card().show(ui, |ui| {
                    style::title(ui, tr(&lang, "Nápověda a data", "Help and data"));
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(tr(&lang, "Otevřít nápovědu", "Open help"))
                            .clicked()
                        {
                            let path = self.data.join(if lang == "cs" {
                                "help.cs.html"
                            } else {
                                "help.en.html"
                            });
                            let text = if lang == "cs" {
                                include_str!("../resources/help.cs.html")
                            } else {
                                include_str!("../resources/help.en.html")
                            };
                            let result = model::atomic_write(&path, text.as_bytes())
                                .and_then(|_| open::that(path).map_err(Into::into))
                                .map(|_| "");
                            self.result(result);
                        }
                        if ui.button(tr(&lang, "Složka dat", "Data folder")).clicked() {
                            self.result(open::that(&self.data).map(|_| "").map_err(Into::into));
                        }
                        if ui
                            .button(tr(&lang, "Licence nástrojů", "Tool licenses"))
                            .clicked()
                        {
                            let path = self.data.join("THIRD-PARTY-LICENSES.txt");
                            let result = model::atomic_write(
                                &path,
                                include_bytes!("../THIRD-PARTY-LICENSES.txt"),
                            )
                            .and_then(|_| open::that(path).map_err(Into::into));
                            self.result(result.map(|_| ""));
                        }
                    });
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "{}: {}",
                                tr(&lang, "Předvolba", "Preset"),
                                self.preset.display()
                            ))
                            .size(12.)
                            .color(MUTED),
                        )
                        .wrap(),
                    );
                });
                ui.add_space(15.);
            });
    }
    fn wizard(&mut self, ui: &mut egui::Ui) {
        let step = self.setup.unwrap_or(0);
        let last_step = if self.builder { 4 } else { 3 };
        let lang = self.settings.language.clone();
        ui.add_space(18.);
        ui.label(
            RichText::new(format!(
                "{}  /  {} {} {} {}",
                if self.builder {
                    tr(&lang, "GENERÁTOR INSTALACE", "SETUP BUILDER")
                } else {
                    tr(&lang, "NASTAVENÍ APLIKACE", "APPLICATION SETUP")
                },
                tr(&lang, "KROK", "STEP"),
                step + 1,
                tr(&lang, "ZE", "OF"),
                last_step + 1
            ))
            .color(LIME)
            .size(12.)
            .strong(),
        );
        ui.add_space(8.);
        let (title, subtitle) = match step {
            0 if self.builder => (
                tr(
                    &lang,
                    "Připravte instalaci pro další počítač",
                    "Prepare another computer's setup",
                ),
                tr(
                    &lang,
                    "Zvolte nastavení pro příjemce. Na tomto počítači se nic nepřenastaví.",
                    "Choose the recipient's preferences. This computer's settings will not change.",
                ),
            ),
            0 => (
                tr(
                    &lang,
                    "Vítejte ve WorkFlow Media",
                    "Welcome to WorkFlow Media",
                ),
                tr(
                    &lang,
                    "Vaše stahování, vaše pravidla.",
                    "Your downloads, your rules.",
                ),
            ),
            1 => (
                tr(&lang, "Zkopírovat. A stáhnout.", "Copy. And download."),
                tr(
                    &lang,
                    "Automatické stahování zapněte jen pro služby, které chcete sledovat.",
                    "Enable automatic downloads for the services you want to monitor.",
                ),
            ),
            2 => (
                tr(&lang, "Vaše výchozí kvalita", "Your default quality"),
                tr(
                    &lang,
                    "Nastavení se uloží do souboru vedle aplikace. Kdykoliv ho později změníte.",
                    "Preferences are saved next to the application. You can change them at any time.",
                ),
            ),
            3 => (
                tr(
                    &lang,
                    "Přihlášení z vašeho prohlížeče",
                    "Sign-in from your browser",
                ),
                tr(
                    &lang,
                    "Volitelné cookies pro služby vyžadující přihlášení.",
                    "Optional cookies for services that require sign-in.",
                ),
            ),
            _ => (
                tr(&lang, "Připraveno k předání", "Ready to share"),
                tr(
                    &lang,
                    "Uložte jeden EXE. Příjemce jej otevře a potvrdí správce; zbytek proběhne automaticky.",
                    "Save one EXE. The recipient opens it and approves the administrator prompt; the rest is automatic.",
                ),
            ),
        };
        ui.heading(title);
        ui.colored_label(MUTED, subtitle);
        ui.add_space(20.);
        let height = (ui.available_height() - 85.).max(100.);
        egui::ScrollArea::vertical()
            .id_salt(("wizard", step))
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                style::card().show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 2.).max(0.));
                    match step {
                        0 => {
                            self.general_settings(ui);
                            ui.add_space(16.);
                            ui.separator();
                            style::checkbox(
                                ui,
                                &mut self.settings.automatic_updates,
                                tr(
                                    &lang,
                                    "Automaticky aktualizovat aplikaci z GitHubu",
                                    "Automatically update the application from GitHub",
                                ),
                            );
                            style::caption(
                                ui,
                                tr(
                                    &lang,
                                    "Podepsané aktualizace se instalují, když je fronta nečinná.",
                                    "Signed updates are installed when the queue is idle.",
                                ),
                            );
                        }
                        1 => self.clipboard_settings(ui),
                        2 => self.quality_settings(ui),
                        3 => self.cookies_settings(ui),
                        _ => self.installer_summary(ui),
                    }
                });
            });
        ui.add_space(12.);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let next_label = if self.builder && step == last_step {
                tr(&lang, "Vytvořit instalační EXE", "Create installer EXE")
            } else {
                tr(
                    &lang,
                    if step == last_step {
                        "Uložit a spustit"
                    } else {
                        "Další"
                    },
                    if step == last_step {
                        "Save and start"
                    } else {
                        "Next"
                    },
                )
            };
            if ui
                .add_enabled_ui(
                    !self.cookies_busy && (!self.builder || self.busy == 0),
                    |ui| style::primary(ui, next_label),
                )
                .inner
                .clicked()
            {
                if step < last_step {
                    self.setup = Some(step + 1);
                } else if self.builder {
                    self.export_installer(ui.ctx());
                } else {
                    match self.save() {
                        Ok(()) => {
                            if self.preview {
                                self.quit(ui.ctx());
                            } else {
                                self.setup = None;
                                self.setup_saved = None;
                                self.engine.start();
                                self.monitor_ready.store(true, Ordering::Relaxed);
                                if let Some(tray) = &mut self.tray {
                                    if let Err(e) = tray.set_language(&self.settings.language) {
                                        self.notice = e.to_string();
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            self.notice =
                                model::localized_error(&self.settings.language, &e.to_string())
                        }
                    }
                }
            }
            if ui
                .add_enabled(step > 0, egui::Button::new(tr(&lang, "Zpět", "Back")))
                .clicked()
            {
                self.setup = Some(step - 1);
            }
            if ui.button(tr(&lang, "Zrušit", "Cancel")).clicked() {
                if self.monitor_ready.load(Ordering::Relaxed) {
                    self.cancel_setup();
                } else {
                    self.quit(ui.ctx());
                }
            }
        });
    }
    fn search_page(&mut self, ui: &mut egui::Ui) {
        let lang = self.settings.language.clone();
        style::title(ui, tr(&lang, "Hledání na FastShare", "Search FastShare"));
        style::card().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            style::caption(ui, tr(&lang, "VYHLEDÁVÁNÍ SOUBORŮ", "FIND FILES"));
            ui.horizontal(|ui| {
                let response = ui.add_sized(
                    [(ui.available_width() - 146.).max(150.), 42.],
                    style::text_field(&mut self.search)
                        .margin(egui::vec2(12., 11.))
                        .hint_text(tr(&lang, "Zadejte název souboru…", "Enter a file name…")),
                );
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui
                    .add_enabled_ui(self.busy == 0, |ui| {
                        style::primary_sized(
                            ui,
                            tr(&lang, "Hledat", "Search"),
                            Vec2::new(136., 42.),
                        )
                    })
                    .inner
                    .clicked()
                    || enter)
                    && self.busy == 0
                {
                    let fast = self.engine.fast.clone();
                    let query = self.search.clone();
                    let tx = self.tx.clone();
                    let ctx = ui.ctx().clone();
                    self.busy += 1;
                    std::thread::spawn(move || {
                        let _ = tx.send(Event::Search(fast.search(&query)));
                        ctx.request_repaint();
                    });
                }
            });
        });
        ui.add_space(12.);
        let rows = self.search_results.clone();
        ui.horizontal(|ui| {
            style::title(ui, tr(&lang, "Výsledky hledání", "Search results"));
            style::caption(ui, &rows.len().to_string());
        });
        if rows.is_empty() {
            ui.add_space(16.);
            style::caption(
                ui,
                tr(
                    &lang,
                    "Zadejte název souboru a spusťte hledání. Vyhledávat můžete i bez přihlášení.",
                    "Enter a file name and search. You can search without signing in.",
                ),
            );
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, 58., rows.len(), |ui, range| {
                for index in range {
                    let r = &rows[index];
                    style::card().inner_margin(9).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width((ui.available_width() - 180.).max(200.));
                                ui.add(egui::Label::new(&r.name).truncate())
                                    .on_hover_text(&r.name);
                                style::caption(
                                    ui,
                                    &format!("{} · {}", model::bytes(r.size as f64), r.resolution),
                                );
                            });
                            if ui.button(tr(&lang, "Stáhnout", "Download")).clicked() {
                                self.result(self.engine.add_presets(&[r.url.clone()]).map(|_| {
                                    tr(&lang, "Přidáno do fronty", "Added to queue").to_owned()
                                }));
                            }
                            if ui
                                .small_button("↗")
                                .on_hover_text(tr(&lang, "Otevřít zdroj", "Open source"))
                                .clicked()
                            {
                                self.result(open::that(&r.url).map(|_| "").map_err(Into::into));
                            }
                        });
                    });
                }
            });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language.clone();
        if self.paste_dialog {
            let mut open = true;
            egui::Window::new(tr(
                &lang,
                "Vložit odkaz ke stažení",
                "Paste a download link",
            ))
            .id("paste-dialog".into())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(600.)
            .show(ctx, |ui| {
                ui.add(
                    style::text_area(&mut self.links)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    format_combo(ui, "paste-format", &mut self.format, false, &lang);
                    quality_combo(ui, "paste-quality", &mut self.quality, false, &lang);
                    if style::primary(ui, tr(&lang, "Přidat do fronty", "Add to queue")).clicked()
                    {
                        self.add_links();
                    }
                });
            });
            self.paste_dialog &= open;
        }
        if let Some(job) = self.info.clone() {
            let mut open = true;
            egui::Window::new(tr(&lang, "Informace o souboru", "File information"))
                .id("file-info".into())
                .open(&mut open)
                .collapsible(false)
                .default_width(620.)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&job.title).strong().size(18.));
                    ui.separator();
                    egui::Grid::new("file-info-fields")
                        .spacing([18., 10.])
                        .show(ui, |ui| {
                            for (label, value) in [
                                (
                                    tr(&lang, "Zdroj", "Source"),
                                    source_label(&job.source, &lang).to_string(),
                                ),
                                (
                                    tr(&lang, "Stav", "Status"),
                                    job.state.label(&lang).to_string(),
                                ),
                                (
                                    tr(&lang, "Velikost", "Size"),
                                    model::bytes(job.total as f64),
                                ),
                                (
                                    tr(&lang, "Formát / kvalita", "Format / quality"),
                                    format!(
                                        "{} / {}",
                                        job.format,
                                        quality_label(job.quality, &lang)
                                    ),
                                ),
                                (
                                    tr(&lang, "Přidáno", "Added"),
                                    job.created.format("%Y-%m-%d %H:%M:%S").to_string(),
                                ),
                                (
                                    tr(&lang, "Dokončeno", "Finished"),
                                    job.finished
                                        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                                        .unwrap_or_else(|| "—".into()),
                                ),
                            ] {
                                ui.colored_label(MUTED, label);
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                    ui.label(tr(&lang, "Umístění", "Location"));
                    let mut path = job.file_path.clone();
                    style::wrapping_field(ui, &mut path, ui.available_width(), "");
                    ui.hyperlink_to(&job.url, &job.url);
                    ui.label(model::message(&lang, &job.message));
                    ui.collapsing(
                        tr(&lang, "Podrobnosti stahování", "Download details"),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(230.)
                                .show(ui, |ui| {
                                    let mut text = job.log.clone();
                                    ui.add(
                                        style::text_area(&mut text)
                                            .font(egui::TextStyle::Monospace)
                                            .desired_width(f32::INFINITY),
                                    );
                                });
                        },
                    );
                });
            if !open {
                self.info = None;
            }
        }
        if self.confirm.is_some() {
            let mut accept = false;
            let mut cancel = false;
            egui::Modal::new("confirmation".into()).show(ctx,|ui|{ui.set_width(470.);style::title(ui,tr(&lang,"Potvrzení","Confirmation"));ui.add_space(8.);let text=match self.confirm.as_ref().unwrap(){Confirm::Clear(_,library)=>tr(&lang,if *library{"Odebrat zvolené záznamy z knihovny? Soubory na disku zůstanou zachované."}else{"Odebrat zvolené dokončené, pozastavené a zrušené záznamy? Knihovna a soubory na disku zůstanou zachované."},if *library{"Remove these library records? Files on disk will be kept."}else{"Remove selected finished, paused and cancelled records? The library and files on disk will be kept."}),Confirm::Recycle(_)=>tr(&lang,"Přesunout soubor do koše? Záznam v knihovně zůstane, ale soubor v původním umístění nebude.","Move this file to the recycle bin? The library record remains, but the file will no longer be at its original location."),Confirm::Quit=>if self.update_file.is_some(){tr(&lang,"Nainstalovat ověřenou aktualizaci a restartovat? Rozpracované úlohy se bezpečně pozastaví.","Install the verified update and restart? Active downloads will be paused safely.")}else{tr(&lang,"Ukončit aplikaci? Rozpracované úlohy se bezpečně pozastaví.","Quit the application? Active downloads will be paused safely.")}};ui.label(text);ui.add_space(14.);ui.horizontal(|ui|{accept=style::primary(ui,tr(&lang,"Potvrdit","Confirm")).clicked();cancel=ui.button(tr(&lang,"Zrušit","Cancel")).clicked();});});
            if accept {
                match self.confirm.take().unwrap() {
                    Confirm::Clear(ids, library) => {
                        let result = self
                            .engine
                            .remove_records(&ids, library)
                            .map(|n| format!("{}: {n}", tr(&lang, "Odebráno", "Removed")));
                        self.result(result);
                        self.selected.clear();
                    }
                    Confirm::Recycle(id) => {
                        let engine = self.engine.clone();
                        let success = tr(
                            &lang,
                            "Soubor přesunut do koše",
                            "File moved to recycle bin",
                        )
                        .to_owned();
                        self.spawn(ctx, move || engine.recycle(&id).map(|_| success));
                    }
                    Confirm::Quit => self.quit(ctx),
                }
            } else if cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.confirm = None;
            }
        }
        if self.quitting {
            egui::Modal::new("shutting-down".into()).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr(
                        &lang,
                        "Ukládám stav a ukončuji přenosy…",
                        "Saving state and stopping transfers…",
                    ));
                });
            });
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.events(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.exit_ready {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.setup.is_none() && self.settings.close_to_tray && self.tray.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            } else {
                self.quit(ctx);
            }
        }
        if self.setup.is_none() {
            self.sidebar(ctx);
        }
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(16, 31, 40))
                    .inner_margin(egui::Margin::symmetric(20, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.busy > 0 {
                        ui.spinner();
                    }
                    ui.add(
                        egui::Label::new(RichText::new(&self.notice).size(12.).color(MUTED))
                            .truncate(),
                    )
                    .on_hover_text(&self.notice);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        style::caption(
                            ui,
                            if self.builder {
                                self.t("Nastavení pro příjemce", "Recipient configuration")
                            } else {
                                match self.engine.snapshot().settings.clipboard_mode {
                                    ClipboardMode::Off => {
                                        self.t("Schránka: vypnuto", "Clipboard: off")
                                    }
                                    ClipboardMode::Selected => self.t(
                                        "Schránka: vybrané služby",
                                        "Clipboard: selected services",
                                    ),
                                    ClipboardMode::All => {
                                        self.t("Schránka: všechny weby", "Clipboard: all sites")
                                    }
                                }
                            },
                        );
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(style::BG)
                    .inner_margin(if self.setup.is_some() { 32 } else { 22 }),
            )
            .show(ctx, |ui| {
                style::background(ui);
                if self.setup.is_some() {
                    self.wizard(ui);
                } else {
                    match self.page {
                        Page::Downloads => self.downloads(ui, false),
                        Page::Library => self.downloads(ui, true),
                        Page::FastShare => self.search_page(ui),
                        Page::Settings => self.settings_page(ui),
                    }
                }
            });
        self.dialogs(ctx);
        ctx.request_repaint_after(Duration::from_millis(
            if self.state.jobs.iter().any(|j| j.state.active()) {
                100
            } else {
                400
            },
        ));
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.stop.store(true, Ordering::Relaxed);
        self.engine.shutdown();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn form_app(data: &std::path::Path) -> App {
        let engine = Engine::new(data, Some(Settings::default())).unwrap();
        let state = engine.snapshot();
        let (tx, rx) = std::sync::mpsc::channel();
        let (_, tray_rx) = std::sync::mpsc::channel();
        App {
            engine,
            settings: state.settings.clone(),
            state,
            page: Page::Settings,
            preset: data.join("preset.json"),
            data: data.into(),
            exe: data.join("app.exe"),
            setup: None,
            preview: true,
            builder: false,
            builder_folder: "WorkFlow Media".into(),
            install_location: crate::installer::Location::default(),
            created_installer: None,
            setup_saved: None,
            cookies_busy: false,
            cookie_epoch: 0,
            cookie_cancel: Arc::new(AtomicBool::new(false)),
            cookie_task: None,
            cookie_results: Vec::new(),
            links: String::new(),
            format: "MP4".into(),
            quality: 0,
            destination: String::new(),
            selected: HashSet::new(),
            anchor: None,
            filter: Filter::default(),
            sort: Sort::Newest,
            notice: String::new(),
            busy: 0,
            tx,
            rx,
            tray_rx,
            tray: None,
            stop: Arc::new(AtomicBool::new(false)),
            monitor_ready: Arc::new(AtomicBool::new(false)),
            snapshot_at: Instant::now(),
            show_signal: None,
            confirm: None,
            info: None,
            paste_dialog: false,
            search: String::new(),
            search_results: vec![],
            login: String::new(),
            password: String::new(),
            update: None,
            update_file: None,
            update_auto: false,
            update_checked: Instant::now(),
            quitting: false,
            exit_ready: false,
        }
    }

    #[test]
    fn settings_and_builder_fields_fit_narrow_and_wide_windows_in_both_languages() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = form_app(dir.path());
        let path = format!("C:\\Example\\{}file.txt", "long-folder-name\\".repeat(12));
        app.settings.download_directory = path.clone();
        app.settings.cookies_file = path.clone();
        for lang in ["cs", "en"] {
            app.settings.language = lang.into();
            for width in [480., 720., 1100.] {
                for section in 0..5 {
                    let ctx = egui::Context::default();
                    style::apply(&ctx);
                    let mut bounds = egui::Rect::NOTHING;
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(width, 1400.),
                        )),
                        ..Default::default()
                    };
                    for _ in 0..2 {
                        let output = ctx.run(input.clone(), |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                match section {
                                    0 => app.general_settings(ui),
                                    1 => app.cookies_settings(ui),
                                    2 => app.installer_locations(ui),
                                    _ => {
                                        app.builder = section == 4;
                                        app.settings.cookies_browser =
                                            Some(model::Browser::Firefox);
                                        app.cookies_settings(ui);
                                        app.settings.cookies_browser = None;
                                        app.builder = false;
                                    }
                                }
                                bounds = ui.min_rect();
                            });
                        });
                        if section == 0 {
                            for shape in output.shapes {
                                if let egui::Shape::Text(text) = shape.shape {
                                    if text.galley.text()
                                        == tr(lang, "Jazyk aplikace", "Application language")
                                    {
                                        assert_eq!(
                                            text.galley.rows.len(),
                                            1,
                                            "Language label split at {width}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                    assert!(
                        bounds.right() <= width + 1.,
                        "section {section}, {lang}, width {width}: {bounds:?}"
                    );
                }
            }
        }
        assert_eq!(app.settings.download_directory, path);
        assert_eq!(app.settings.cookies_file, path);
    }

    #[test]
    fn browser_page_is_available_in_both_setup_guides_and_languages() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = form_app(dir.path());
        for lang in ["cs", "en"] {
            for builder in [false, true] {
                app.builder = builder;
                app.setup = Some(3);
                app.settings.language = lang.into();
                app.settings.cookies_mode = model::CookieMode::All;
                let ctx = egui::Context::default();
                style::apply(&ctx);
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1100., 850.),
                    )),
                    ..Default::default()
                };
                let mut text = String::new();
                for _ in 0..2 {
                    let out = ctx.run(input.clone(), |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| app.wizard(ui));
                    });
                    text = out
                        .shapes
                        .into_iter()
                        .filter_map(|s| {
                            if let egui::Shape::Text(t) = s.shape {
                                Some(t.galley.text().to_owned())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                }
                assert!(text.contains("Mozilla Firefox"));
                assert!(text.contains(tr(lang, "Používat cookies", "Use browser cookies")));
                assert!(text.contains(tr(
                    lang,
                    "Vytáhnout všechny cookies",
                    "Load all browser cookies"
                )));
                assert!(text.contains(tr(
                    lang,
                    "Automaticky aktualizovat cookies profilů",
                    "Automatically refresh profile cookies"
                )));
                assert!(text.contains(if builder {
                    tr(lang, "Další", "Next")
                } else {
                    tr(lang, "Uložit a spustit", "Save and start")
                }));
            }
        }
    }
    #[test]
    fn eight_profile_controls_fit_without_horizontal_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = form_app(dir.path());
        app.settings.cookie_profiles = (0..8)
            .map(|i| model::CookieProfile {
                browser: if i < 6 {
                    model::Browser::Chrome
                } else {
                    model::Browser::Brave
                },
                path: dir
                    .path()
                    .join(format!("Profile {i}"))
                    .display()
                    .to_string(),
                label: format!("Account {i} · {}", "long-profile-name-".repeat(4)),
                enabled: true,
            })
            .collect();
        for lang in ["cs", "en"] {
            app.settings.language = lang.into();
            for width in [480., 720., 1100.] {
                for mode in [model::CookieMode::Selected, model::CookieMode::All] {
                    app.settings.cookies_mode = mode;
                    let ctx = egui::Context::default();
                    style::apply(&ctx);
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(width, 2200.),
                        )),
                        ..Default::default()
                    };
                    let mut bounds = egui::Rect::NOTHING;
                    for _ in 0..2 {
                        let _ = ctx.run(input.clone(), |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                app.cookies_settings(ui);
                                bounds = ui.min_rect();
                            });
                        });
                    }
                    assert!(
                        bounds.right() <= width + 1.,
                        "{mode:?}, {lang}, {width}: {bounds:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn cancelled_setup_ignores_late_cookie_results_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = form_app(dir.path());
        app.begin_setup();
        app.cookies_busy = true;
        app.busy = 1;
        app.cookie_cancel = Arc::new(AtomicBool::new(false));
        let cancelled = app.cookie_cancel.clone();
        let epoch = app.cookie_epoch;
        assert!(app.save().is_err());
        app.settings.cookies_mode = model::CookieMode::All;
        app.cancel_setup();
        assert!(cancelled.load(Ordering::Relaxed));
        assert_eq!(app.settings.cookies_mode, model::CookieMode::Single);
        app.begin_setup();
        app.tx
            .send(Event::Cookies(
                Ok(crate::cookies::RefreshReport {
                    profiles: vec![model::CookieProfile {
                        browser: model::Browser::Chrome,
                        path: String::new(),
                        label: "Discarded result".into(),
                        enabled: true,
                    }],
                    results: Vec::new(),
                }),
                false,
                epoch,
            ))
            .unwrap();
        app.events(&egui::Context::default());
        assert!(app.settings.cookie_profiles.is_empty());
        assert!(!app.cookies_busy);
        assert_eq!(app.busy, 0);
    }
    #[test]
    fn quitting_waits_for_cookie_worker_to_clean_up() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = form_app(dir.path());
        let cookie = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        let path = cookie.path().to_owned();
        let cancel = app.cookie_cancel.clone();
        app.cookie_task = Some(std::thread::spawn(move || {
            while !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(cookie);
        }));
        let ctx = egui::Context::default();
        app.quit(&ctx);
        let started = Instant::now();
        while !app.exit_ready && started.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(10));
            app.events(&ctx);
        }
        assert!(app.exit_ready);
        assert!(!path.exists());
    }
    #[test]
    fn display_sort_does_not_change_queue() {
        let a = Job {
            title: "a".into(),
            percent: 9.,
            ..Job::default()
        };
        let b = Job {
            title: "b".into(),
            percent: 80.,
            ..Job::default()
        };
        let original = vec![a.clone(), b.clone()];
        let mut display = original.clone();
        sort_jobs(&mut display, Sort::ProgressDesc);
        assert_eq!(display[0].id, b.id);
        assert_eq!(original[0].id, a.id);
        sort_jobs(&mut display, Sort::Newest);
        assert_eq!(display[0].id, b.id);
    }
}
