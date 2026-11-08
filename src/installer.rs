use crate::{
    model::{self, ClipboardMode, Preference, Preset, Settings, tr},
    platform, style, updater,
};
use anyhow::{Context, Result, bail};
use eframe::egui;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

const MAGIC: &[u8; 16] = b"WFMEDIA_BUNDLE_1";
const FOOTER: u64 = 56;
const MAX_PROFILE: u64 = 65536;
/// The email attachment must remain a downloader, never a copy of the media application.
#[cfg(any(bundled_installer, test))]
pub const MAX_INSTALLER_BYTES: u64 = 5 * 1024 * 1024;
#[cfg(all(windows, feature = "setup-stub"))]
#[path = "installer/compact.rs"]
pub mod compact;
const APPLICATION: &str = "WorkFlow Media.exe";
const PRESET: &str = "WorkFlow Media.preset.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserFolder {
    Documents,
    Desktop,
    Downloads,
    Programs,
    StartMenu,
}
impl UserFolder {
    pub fn label(self, lang: &str) -> &str {
        match self {
            Self::Documents => tr(lang, "Dokumenty", "Documents"),
            Self::Desktop => tr(lang, "Plocha", "Desktop"),
            Self::Downloads => tr(lang, "Stažené", "Downloads"),
            Self::Programs => tr(lang, "Místní aplikace uživatele", "Local user applications"),
            Self::StartMenu => tr(lang, "Nabídka Start", "Start menu"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Location {
    pub folder: UserFolder,
    pub subfolder: String,
    pub shortcut: Option<UserFolder>,
}
impl Default for Location {
    fn default() -> Self {
        Self {
            folder: UserFolder::Documents,
            subfolder: "WorkFlow Media".into(),
            shortcut: Some(UserFolder::Desktop),
        }
    }
}

/// Resolve special folders on the recipient's computer, including redirected/OneDrive folders.
struct RecipientFolders {
    documents: Option<PathBuf>,
    desktop: Option<PathBuf>,
    downloads: Option<PathBuf>,
    programs: PathBuf,
    start_menu: PathBuf,
}
impl RecipientFolders {
    fn current() -> Result<Self> {
        let user = directories::UserDirs::new()
            .context("User folders unavailable / Složky uživatele nejsou dostupné")?;
        let base = directories::BaseDirs::new()
            .context("User profile unavailable / Profil uživatele není dostupný")?;
        Ok(Self {
            documents: user.document_dir().map(Path::to_path_buf),
            desktop: user.desktop_dir().map(Path::to_path_buf),
            downloads: user.download_dir().map(Path::to_path_buf),
            programs: base.data_local_dir().join("Programs"),
            start_menu: base
                .config_dir()
                .join("Microsoft/Windows/Start Menu/Programs"),
        })
    }
    fn get(&self, folder: UserFolder) -> Result<&Path> {
        match folder {
            UserFolder::Documents => self.documents.as_deref(),
            UserFolder::Desktop => self.desktop.as_deref(),
            UserFolder::Downloads => self.downloads.as_deref(),
            UserFolder::Programs => Some(self.programs.as_path()),
            UserFolder::StartMenu => Some(self.start_menu.as_path()),
        }
        .with_context(|| {
            format!(
                "Recipient folder unavailable: {} / Složka příjemce není dostupná: {}",
                folder.label("en"),
                folder.label("cs")
            )
        })
    }
}

/// An explicit allowlist: no credentials, cookies, local paths or queue/library data.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Choices {
    #[serde(default)]
    pub cookies_mode: model::CookieMode,
    #[serde(default = "model::default_cookie_refresh")]
    pub cookies_auto_refresh: bool,
    #[serde(default = "model::default_cookie_browsers")]
    pub cookies_browsers: Vec<model::Browser>,
    #[serde(default)]
    pub cookies_browser: Option<model::Browser>,
    pub language: String,
    #[serde(default)]
    pub theme: model::Theme,
    pub parallel: usize,
    pub format: String,
    pub quality: i32,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub start_on_add: bool,
    pub clipboard_mode: ClipboardMode,
    pub clipboard_sources: Vec<String>,
    pub service_preferences: BTreeMap<String, Preference>,
    pub automatic_updates: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub download_folder: String,
    pub location: Location,
    pub choices: Choices,
}
impl Profile {
    pub fn from_settings(s: &Settings, folder: String, location: Location) -> Result<Self> {
        let profile = Self {
            version: 1,
            download_folder: folder,
            location,
            choices: Choices {
                cookies_browser: s.cookies_browser,
                cookies_mode: if s.cookies_mode == model::CookieMode::Selected {
                    model::CookieMode::All
                } else {
                    s.cookies_mode
                },
                cookies_auto_refresh: s.cookies_auto_refresh,
                cookies_browsers: if s.cookies_mode == model::CookieMode::Selected {
                    let mut browsers = Vec::new();
                    for p in s.cookie_profiles.iter().filter(|p| p.enabled) {
                        if !browsers.contains(&p.browser) {
                            browsers.push(p.browser);
                        }
                    }
                    browsers
                } else {
                    s.cookies_browsers.clone()
                },
                language: s.language.clone(),
                theme: s.theme,
                parallel: s.parallel,
                format: s.format.clone(),
                quality: s.quality,
                start_with_windows: s.start_with_windows,
                close_to_tray: s.close_to_tray,
                start_on_add: s.start_on_add,
                clipboard_mode: s.clipboard_mode,
                clipboard_sources: s.clipboard_sources.clone(),
                service_preferences: s.service_preferences.clone(),
                automatic_updates: s.automatic_updates,
            },
        };
        profile.resolve(&std::env::temp_dir())?;
        Ok(profile)
    }
    pub fn resolve(&self, downloads: &Path) -> Result<Settings> {
        let valid_folder = |folder: &str| {
            !folder.is_empty() && folder.len() <= 120 && model::safe_name(folder) == folder
        };
        if self.version != 1
            || !valid_folder(&self.download_folder)
            || !valid_folder(&self.location.subfolder)
            || self.location.folder == UserFolder::StartMenu
            || self.location.shortcut == Some(UserFolder::Programs)
        {
            bail!("Invalid recipient folder / Neplatná složka příjemce");
        }
        let c = &self.choices;
        if c.cookies_mode == model::CookieMode::Selected {
            bail!(
                "Installer profiles must use portable browser choices / Instalační profily musí používat přenositelné volby prohlížečů"
            );
        }
        let s = Settings {
            cookies_browser: c.cookies_browser,
            cookies_mode: c.cookies_mode,
            cookies_auto_refresh: c.cookies_auto_refresh,
            cookies_browsers: c.cookies_browsers.clone(),
            download_directory: downloads
                .join(&self.download_folder)
                .to_string_lossy()
                .into_owned(),
            language: c.language.clone(),
            theme: c.theme,
            parallel: c.parallel,
            format: c.format.clone(),
            quality: c.quality,
            start_with_windows: c.start_with_windows,
            close_to_tray: c.close_to_tray,
            start_on_add: c.start_on_add,
            clipboard_mode: c.clipboard_mode,
            clipboard_sources: c.clipboard_sources.clone(),
            service_preferences: c.service_preferences.clone(),
            automatic_updates: c.automatic_updates,
            ..Settings::default()
        };
        s.validate()?;
        Ok(s)
    }
    fn installation_paths(&self, folders: &RecipientFolders) -> Result<(PathBuf, Option<PathBuf>)> {
        self.resolve(folders.get(UserFolder::Downloads)?)?;
        let root = folders
            .get(self.location.folder)?
            .join(&self.location.subfolder);
        let shortcut = self
            .location
            .shortcut
            .map(|folder| folders.get(folder).map(|p| p.join("WorkFlow Media.lnk")))
            .transpose()?;
        Ok((root, shortcut))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "Mode", content = "Profile", deny_unknown_fields)]
pub enum Package {
    Builder,
    Installer(Profile),
}

pub fn read_package(path: &Path) -> Result<Option<(u64, Package)>> {
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    if length < FOOTER {
        return Ok(None);
    }
    file.seek(SeekFrom::End(-(FOOTER as i64)))?;
    let mut footer = [0u8; FOOTER as usize];
    file.read_exact(&mut footer)?;
    if &footer[40..] != MAGIC {
        return Ok(None);
    }
    let size = u64::from_le_bytes(footer[..8].try_into()?);
    if size == 0 || size > MAX_PROFILE {
        bail!("Invalid installer profile length");
    }
    let base = length
        .checked_sub(FOOTER + size)
        .filter(|v| *v > 0)
        .context("Truncated installer")?;
    file.seek(SeekFrom::Start(base))?;
    let mut payload = vec![0; size as usize];
    file.read_exact(&mut payload)?;
    if Sha256::digest(&payload)[..] != footer[8..40] {
        bail!("Installer profile is damaged / Nastavení instalátoru je poškozené");
    }
    let package = serde_json::from_slice(&payload)?;
    if let Package::Installer(ref p) = package {
        p.resolve(&std::env::temp_dir())?;
    }
    Ok(Some((base, package)))
}

/// PE executables permit an overlay. Repackaging strips any previous overlay first.
fn write_package(source: &Path, output: &Path, package: &Package) -> Result<()> {
    if output.exists() && fs::canonicalize(source)? == fs::canonicalize(output)? {
        bail!("Choose a different output file / Vyberte jiný výstupní soubor");
    }
    let base = read_package(source)?
        .map(|p| p.0)
        .unwrap_or(fs::metadata(source)?.len());
    write_overlay(fs::File::open(source)?.take(base), output, package)
}

fn write_overlay(mut source: impl Read, output: &Path, package: &Package) -> Result<()> {
    if let Package::Installer(p) = package {
        p.resolve(&std::env::temp_dir())?;
    }
    let payload = serde_json::to_vec(package)?;
    if payload.len() as u64 > MAX_PROFILE {
        bail!("Installer profile too large");
    }
    let output = std::path::absolute(output)?;
    let parent = output.parent().context("Output folder")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut source, &mut temp)?;
    temp.write_all(&payload)?;
    temp.write_all(&(payload.len() as u64).to_le_bytes())?;
    temp.write_all(&Sha256::digest(&payload))?;
    temp.write_all(MAGIC)?;
    temp.as_file().sync_all()?;
    platform::replace_file(temp.path(), &output)?;
    Ok(())
}

#[cfg(any(bundled_installer, test))]
fn package_installer(stub: &[u8], output: &Path, profile: &Profile) -> Result<()> {
    // Leave room for the largest permitted preset, so future choices cannot grow the EXE.
    if !stub.starts_with(b"MZ") || stub.len() as u64 > MAX_INSTALLER_BYTES - MAX_PROFILE - FOOTER {
        bail!("Invalid or oversized setup stub; rebuild using build-windows.ps1");
    }
    if output.exists() && fs::canonicalize(std::env::current_exe()?)? == fs::canonicalize(output)? {
        bail!("Choose a different output file / Vyberte jiný výstupní soubor");
    }
    write_overlay(stub, output, &Package::Installer(profile.clone()))
}

/// Embed only the small Rust downloader and the portable preset in the recipient's EXE.
pub fn create_installer(output: &Path, profile: &Profile) -> Result<()> {
    #[cfg(bundled_installer)]
    return package_installer(
        include_bytes!("../resources/setup-stub.exe"),
        output,
        profile,
    );
    #[cfg(not(bundled_installer))]
    {
        let _ = (output, profile);
        bail!(
            "Setup downloader missing / Chybí instalační stahovač. Build using build-windows.ps1"
        );
    }
}

pub fn export_builder(output: &Path) -> Result<()> {
    if !cfg!(windows) {
        bail!("The Windows installer builder must be built and run on Windows");
    }
    write_package(&std::env::current_exe()?, output, &Package::Builder)
}

fn restore(path: &Path, previous: Option<&[u8]>) -> Result<()> {
    if let Some(bytes) = previous {
        model::atomic_write(path, bytes)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// The caller holds the application lock; failed writes restore the previous installation.
fn commit_install(
    download: &Path,
    root: &Path,
    mut settings: Settings,
    register: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    fs::create_dir_all(root)?;
    let exe = root.join(APPLICATION);
    let preset = root.join(PRESET);
    let previous_exe = if exe.exists() {
        Some(fs::read(&exe)?)
    } else {
        None
    };
    let previous_preset = if preset.exists() {
        Some(fs::read(&preset)?)
    } else {
        None
    };
    if let Some(bytes) = &previous_preset {
        let old: Preset = serde_json::from_slice(bytes)
            .context("Existing preset is unreadable; installation left unchanged")?;
        settings.cookies_file = old.settings.cookies_file;
        settings.cookie_profiles = old.settings.cookie_profiles;
        settings.hidden_columns = old.settings.hidden_columns;
    }
    let mut stage = tempfile::NamedTempFile::new_in(root)?;
    std::io::copy(&mut fs::File::open(download)?, &mut stage)?;
    stage.as_file().sync_all()?;
    let stage = stage.into_temp_path();
    let result = (|| -> Result<()> {
        platform::replace_file(&stage, &exe)?;
        model::save_json(
            &preset,
            &Preset {
                setup_completed: true,
                settings,
                ..Preset::default()
            },
        )?;
        register(&exe)?;
        Ok(())
    })();
    if let Err(error) = result {
        restore(&exe, previous_exe.as_deref()).context("Cannot restore previous application")?;
        restore(&preset, previous_preset.as_deref()).context("Cannot restore previous preset")?;
        return Err(error);
    }
    Ok(exe)
}

#[derive(Clone, Copy)]
enum Stage {
    Authorize,
    Verify,
    Download,
    Install,
    Launch,
}
impl Stage {
    fn label(self, lang: &str) -> &str {
        match self {
            Self::Authorize => tr(
                lang,
                "Potvrďte oprávnění správce…",
                "Confirm the administrator prompt…",
            ),
            Self::Verify => tr(
                lang,
                "Ověřuji vydání na GitHubu…",
                "Verifying the GitHub release…",
            ),
            Self::Download => tr(
                lang,
                "Stahuji a ověřuji aplikaci…",
                "Downloading and verifying the application…",
            ),
            Self::Install => tr(
                lang,
                "Ukládám aplikaci a připravené nastavení…",
                "Installing the application and prepared settings…",
            ),
            Self::Launch => tr(
                lang,
                "Spouštím připravenou aplikaci…",
                "Starting your configured application…",
            ),
        }
    }
}
struct Installed {
    exe: PathBuf,
    warning: String,
}
enum Event {
    Stage(Stage),
    Done(Result<Installed>),
}

fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("Installation cancelled / Instalace byla zrušena");
    }
    Ok(())
}

fn install(
    profile: &Profile,
    test_root: Option<&Path>,
    cancel: &AtomicBool,
    report: impl Fn(Stage),
) -> Result<Installed> {
    if !cfg!(windows) {
        bail!("This installer targets Windows x64");
    }
    let test = test_root.is_some();
    if !test {
        report(Stage::Authorize);
        platform::authorize_setup()?;
    }
    cancelled(cancel)?;
    let (root, shortcut, downloads) = if let Some(root) = test_root {
        (root.to_path_buf(), None, root.join("Downloads"))
    } else {
        let folders = RecipientFolders::current()?;
        let (root, shortcut) = profile.installation_paths(&folders)?;
        (
            root,
            shortcut,
            folders.get(UserFolder::Downloads)?.to_path_buf(),
        )
    };
    let data = if test {
        root.join("data")
    } else {
        platform::app_data()
    };
    fs::create_dir_all(&data)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data.join("instance.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock).context("Close WorkFlow Media using Quit in its tray menu, then retry / Ukončete WorkFlow Media volbou Ukončit u hodin a zkuste to znovu")?;
    let settings = profile.resolve(&downloads)?;
    report(Stage::Verify);
    let manifest = updater::latest()?;
    cancelled(cancel)?;
    report(Stage::Download);
    let cache = tempfile::tempdir()?;
    let download = updater::download(&manifest, cache.path())?;
    cancelled(cancel)?;
    report(Stage::Install);
    fs::create_dir_all(&settings.download_directory)?;
    let startup = settings.start_with_windows;
    let exe = commit_install(&download, &root, settings, |exe| {
        if test {
            Ok(())
        } else {
            platform::set_startup(startup, exe)
        }
    })?;
    let mut warning = String::new();
    #[cfg(windows)]
    if let Some(shortcut) = shortcut {
        if let Err(e) = platform::create_shortcut(&exe, &shortcut) {
            warning = format!("Cannot create shortcut / Nelze vytvořit zástupce: {e}");
        }
    }
    #[cfg(not(windows))]
    let _ = shortcut;
    drop(lock);
    if !test && !cancel.load(Ordering::Relaxed) {
        report(Stage::Launch);
        if let Err(e) = platform::command(&exe).spawn() {
            warning =
                format!("Open the installed application / Otevřete nainstalovanou aplikaci: {e}");
        }
    }
    Ok(Installed { exe, warning })
}

/// Exercises signed downloads and installation in an explicit isolated directory, without UAC,
/// startup registration, shortcuts or launching another process.
pub fn test_install(profile: &Profile, root: &Path) -> Result<PathBuf> {
    if !root.is_absolute() {
        bail!("An absolute test directory is required");
    }
    Ok(install(profile, Some(root), &AtomicBool::new(false), |_| {})?.exe)
}

pub fn run(profile: Profile) -> Result<()> {
    profile.resolve(&std::env::temp_dir())?;
    let icon = image::load_from_memory(include_bytes!("../assets/workflow.png"))?.into_rgba8();
    let (width, height) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("WorkFlow Media · Setup")
            .with_inner_size([700., 430.])
            .with_min_inner_size([620., 390.])
            .with_icon(egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            }),
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "WorkFlow Media Setup",
        options,
        Box::new(move |cc| {
            style::apply(&cc.egui_ctx, profile.choices.theme);
            Ok(Box::new(InstallerUi::new(profile, cc.egui_ctx.clone())))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
struct InstallerUi {
    profile: Profile,
    rx: mpsc::Receiver<Event>,
    cancel: Arc<AtomicBool>,
    stage: Stage,
    result: Option<Result<Installed>>,
    closing: bool,
}
impl InstallerUi {
    fn new(profile: Profile, ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_profile = profile.clone();
        std::thread::spawn(move || {
            let result = install(&worker_profile, None, &worker_cancel, |stage| {
                let _ = tx.send(Event::Stage(stage));
                ctx.request_repaint();
            });
            let _ = tx.send(Event::Done(result));
            ctx.request_repaint();
        });
        Self {
            profile,
            rx,
            cancel,
            stage: Stage::Authorize,
            result: None,
            closing: false,
        }
    }
}
impl eframe::App for InstallerUi {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Stage(s) => self.stage = s,
                Event::Done(result) => self.result = Some(result),
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.result.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.cancel.store(true, Ordering::Relaxed);
            self.closing = true;
        }
        if self.closing && self.result.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let lang = self.profile.choices.language.clone();
        let c = style::palette(self.profile.choices.theme);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(c.bg).inner_margin(28)).show(ctx, |ui| {
            style::background(ui);
            ui.heading(tr(&lang, "Instalace WorkFlow Media", "Install WorkFlow Media"));
            ui.add_space(12.);
            style::caption(ui, tr(&lang, "Aplikace se stáhne z GitHubu a spustí s připraveným nastavením.", "The application will download from GitHub and start with your prepared settings."));
            ui.add_space(20.);
            style::card(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                if let Some(result) = &self.result {
                    match result {
                        Ok(installed) => {
                            ui.colored_label(c.accent, tr(&lang, "Hotovo. Aplikace je připravená.", "Done. Your application is ready."));
                            ui.add(egui::Label::new(installed.exe.display().to_string()).wrap());
                            if !installed.warning.is_empty() { ui.add(egui::Label::new(&installed.warning).wrap()); }
                            if ui.button(tr(&lang, "Otevřít aplikaci", "Open application")).clicked() { let _ = platform::command(&installed.exe).spawn(); }
                        }
                        Err(e) => { ui.add(egui::Label::new(egui::RichText::new(model::localized_error(&lang, &format!("{e:#}"))).color(c.red)).wrap()); }
                    }
                } else {
                    ui.horizontal(|ui| { ui.spinner(); ui.label(if self.closing { tr(&lang, "Ukončuji instalaci…", "Stopping installation…") } else { self.stage.label(&lang) }); });
                    ui.add_space(12.);
                    ui.add(egui::ProgressBar::new(match self.stage { Stage::Authorize => 0.05, Stage::Verify => 0.2, Stage::Download => 0.45, Stage::Install => 0.8, Stage::Launch => 0.95 }).animate(true));
                }
            });
            ui.add_space(18.);
            if self.result.is_some() {
                if self.result.as_ref().is_some_and(|r| r.is_err()) && ui.button(tr(&lang, "Zkusit znovu", "Try again")).clicked() { *self = Self::new(self.profile.clone(), ctx.clone()); }
                if style::primary(ui, tr(&lang, "Zavřít", "Close")).clicked() { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
            } else if ui.button(tr(&lang, "Zrušit", "Cancel")).clicked() {
                self.cancel.store(true, Ordering::Relaxed); self.closing = true;
            }
        });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipient_locations_use_resolved_folders_and_reject_path_escape() {
        let dir = tempfile::tempdir().unwrap();
        let folders = RecipientFolders {
            documents: Some(dir.path().join("different-user/OneDrive/Dokumenty")),
            desktop: Some(dir.path().join("redirected-drive/Plocha")),
            downloads: Some(dir.path().join("different-user/Downloads")),
            programs: dir.path().join("different-user/Local/Programs"),
            start_menu: dir.path().join("different-user/Roaming/Start Menu"),
        };
        let mut p =
            Profile::from_settings(&Settings::default(), "Media".into(), Location::default())
                .unwrap();
        let (root, shortcut) = p.installation_paths(&folders).unwrap();
        assert_eq!(
            root,
            folders.documents.as_ref().unwrap().join("WorkFlow Media")
        );
        assert_eq!(
            shortcut.unwrap(),
            folders.desktop.as_ref().unwrap().join("WorkFlow Media.lnk")
        );
        for folder in [
            UserFolder::Documents,
            UserFolder::Desktop,
            UserFolder::Downloads,
            UserFolder::Programs,
        ] {
            p.location.folder = folder;
            assert_eq!(
                p.installation_paths(&folders).unwrap().0,
                folders.get(folder).unwrap().join("WorkFlow Media")
            );
        }
        p.location.shortcut = None;
        assert!(p.installation_paths(&folders).unwrap().1.is_none());
        p.location.subfolder = "../escape".into();
        assert!(p.installation_paths(&folders).is_err());
    }
    #[test]
    fn profile_is_portable_and_contains_no_private_settings() {
        let mut s = Settings::default();
        s.download_directory = "PRIVATE_ORIGINAL_PATH".into();
        s.cookies_file = "PRIVATE_COOKIE_PATH".into();
        s.cookies_browser = Some(model::Browser::Firefox);
        s.queue_paused = true;
        s.parallel = 16;
        s.start_with_windows = true;
        let p = Profile::from_settings(&s, "Family videos".into(), Location::default()).unwrap();
        let text = serde_json::to_string(&p).unwrap();
        assert!(!text.contains("PRIVATE_"));
        assert!(!text.contains("CookiesFile"));
        let dir = tempfile::tempdir().unwrap();
        let resolved = p.resolve(dir.path()).unwrap();
        assert_eq!(resolved.cookies_browser, Some(model::Browser::Firefox));
        let mut legacy: serde_json::Value = serde_json::from_str(&text).unwrap();
        legacy["Choices"]
            .as_object_mut()
            .unwrap()
            .remove("CookiesBrowser");
        assert!(
            serde_json::from_value::<Profile>(legacy)
                .unwrap()
                .choices
                .cookies_browser
                .is_none()
        );
        assert_eq!(
            Path::new(&resolved.download_directory),
            dir.path().join("Family videos")
        );
        assert_eq!(resolved.parallel, 16);
        assert!(resolved.start_with_windows);
        assert!(!resolved.queue_paused);
        for folder in ["../escape", "C:\\private", "a/b", "CON", "", ".."] {
            let mut invalid = p.clone();
            invalid.download_folder = folder.into();
            assert!(invalid.resolve(dir.path()).is_err());
        }
    }
    #[test]
    fn multiple_browser_preferences_are_portable_and_legacy_profiles_still_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            cookies_mode: model::CookieMode::Selected,
            cookies_auto_refresh: false,
            cookie_profiles: [
                model::Browser::Chrome,
                model::Browser::Brave,
                model::Browser::Chrome,
            ]
            .into_iter()
            .enumerate()
            .map(|(n, browser)| model::CookieProfile {
                browser,
                path: dir
                    .path()
                    .join(format!("PRIVATE_{n}"))
                    .display()
                    .to_string(),
                label: format!("PRIVATE_account_{n}"),
                enabled: true,
            })
            .collect(),
            ..Settings::default()
        };
        settings.cookie_profiles.push(model::CookieProfile {
            browser: model::Browser::Firefox,
            path: "PRIVATE_DISABLED".into(),
            label: "PRIVATE_DISABLED".into(),
            enabled: false,
        });
        let exported =
            Profile::from_settings(&settings, "Media".into(), Location::default()).unwrap();
        let text = serde_json::to_string(&exported).unwrap();
        assert!(!text.contains("PRIVATE"));
        assert!(!text.contains("CookieProfiles"));
        let loaded = exported.resolve(dir.path()).unwrap();
        assert_eq!(loaded.cookies_mode, model::CookieMode::All);
        assert_eq!(
            loaded.cookies_browsers,
            vec![model::Browser::Chrome, model::Browser::Brave]
        );
        assert!(!loaded.cookies_auto_refresh);
        assert!(loaded.cookie_profiles.is_empty());
        let mut legacy = serde_json::to_value(exported).unwrap();
        for key in ["CookiesMode", "CookiesAutoRefresh", "CookiesBrowsers"] {
            legacy["Choices"].as_object_mut().unwrap().remove(key);
        }
        let legacy: Profile = serde_json::from_value(legacy).unwrap();
        let loaded = legacy.resolve(dir.path()).unwrap();
        assert_eq!(loaded.cookies_mode, model::CookieMode::Single);
        assert!(loaded.cookies_auto_refresh);
    }
    #[test]
    fn package_round_trip_replaces_overlay_and_detects_damage() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.exe");
        fs::write(&base, b"MZ-test-program").unwrap();
        let builder = dir.path().join("builder.exe");
        write_package(&base, &builder, &Package::Builder).unwrap();
        let setup = dir.path().join("setup.exe");
        let p = Profile::from_settings(&Settings::default(), "Media".into(), Location::default())
            .unwrap();
        write_package(&builder, &setup, &Package::Installer(p)).unwrap();
        let (length, package) = read_package(&setup).unwrap().unwrap();
        assert_eq!(length, 15);
        assert!(matches!(package, Package::Installer(_)));
        assert!(write_package(&setup, &setup, &Package::Builder).is_err());
        let mut bytes = fs::read(&setup).unwrap();
        bytes[length as usize] ^= 1;
        fs::write(&setup, &bytes).unwrap();
        assert!(read_package(&setup).is_err());
        let end = bytes.len();
        bytes[end - FOOTER as usize..end - FOOTER as usize + 8]
            .copy_from_slice(&u64::MAX.to_le_bytes());
        fs::write(&setup, bytes).unwrap();
        assert!(read_package(&setup).is_err());
    }
    #[test]
    fn theme_survives_preset_and_recipient_profile_with_dark_legacy_default() {
        let dir = tempfile::tempdir().unwrap();
        for theme in [model::Theme::Dark, model::Theme::Light] {
            let settings = Settings {
                theme,
                ..Default::default()
            };
            let path = dir.path().join(PRESET);
            model::save_json(
                &path,
                &Preset {
                    settings: settings.clone(),
                    setup_completed: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let preset: Preset = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(preset.settings.theme, theme);
            let profile =
                Profile::from_settings(&settings, "Media".into(), Location::default()).unwrap();
            let bytes = serde_json::to_vec(&profile).unwrap();
            let loaded: Profile = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(loaded.resolve(dir.path()).unwrap().theme, theme);
            let mut legacy = serde_json::to_value(profile).unwrap();
            legacy["Choices"].as_object_mut().unwrap().remove("Theme");
            assert_eq!(
                serde_json::from_value::<Profile>(legacy)
                    .unwrap()
                    .resolve(dir.path())
                    .unwrap()
                    .theme,
                model::Theme::Dark
            );
        }
        assert_eq!(
            serde_json::from_str::<Settings>("{}").unwrap().theme,
            model::Theme::Dark
        );
    }

    #[test]
    fn recipient_installer_is_only_the_stub_and_portable_profile() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("setup.exe");
        let profile =
            Profile::from_settings(&Settings::default(), "Media".into(), Location::default())
                .unwrap();
        let stub = b"MZ-small-downloader";
        package_installer(stub, &output, &profile).unwrap();
        let first = fs::read(&output).unwrap();
        let (base, package) = read_package(&output).unwrap().unwrap();
        assert_eq!(base, stub.len() as u64);
        assert_eq!(&first[..base as usize], stub);
        assert!(matches!(package, Package::Installer(_)));
        assert!(first.len() as u64 <= MAX_INSTALLER_BYTES);
        // Repeated exports do not accumulate old application/profile bytes.
        package_installer(stub, &output, &profile).unwrap();
        assert_eq!(fs::read(&output).unwrap(), first);
        assert!(package_installer(b"not-an-executable", &output, &profile).is_err());
        let mut oversized = vec![0; (MAX_INSTALLER_BYTES - MAX_PROFILE - FOOTER + 1) as usize];
        oversized[..2].copy_from_slice(b"MZ");
        assert!(package_installer(&oversized, &output, &profile).is_err());
        assert_eq!(fs::read(&output).unwrap(), first);
        #[cfg(bundled_installer)]
        {
            create_installer(&output, &profile).unwrap();
            let (base, _) = read_package(&output).unwrap().unwrap();
            assert_eq!(
                base,
                include_bytes!("../resources/setup-stub.exe").len() as u64
            );
            assert!(fs::metadata(&output).unwrap().len() <= MAX_INSTALLER_BYTES);
            assert!(create_installer(&std::env::current_exe().unwrap(), &profile).is_err());
        }
        #[cfg(not(bundled_installer))]
        assert!(create_installer(&output, &profile).is_err());
    }

    #[test]
    fn install_writes_completed_preset_and_rolls_back_failure() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("installed");
        let payload = dir.path().join("download.exe");
        fs::write(&payload, b"new-executable").unwrap();
        let choices = Settings {
            cookies_browser: Some(model::Browser::Brave),
            ..Settings::default()
        };
        let p = Profile::from_settings(&choices, "Videos".into(), Location::default()).unwrap();
        let s = p.resolve(dir.path()).unwrap();
        commit_install(&payload, &root, s.clone(), |_| Ok(())).unwrap();
        let preset: Preset = serde_json::from_slice(&fs::read(root.join(PRESET)).unwrap()).unwrap();
        assert!(preset.setup_completed);
        assert_eq!(preset.settings.cookies_browser, Some(model::Browser::Brave));
        fs::write(root.join(APPLICATION), b"old-executable").unwrap();
        let before = fs::read(root.join(PRESET)).unwrap();
        let error =
            commit_install(&payload, &root, s, |_| bail!("Registration failed")).unwrap_err();
        assert!(
            error.to_string().contains("Registration failed"),
            "{error:#}"
        );
        assert_eq!(fs::read(root.join(APPLICATION)).unwrap(), b"old-executable");
        assert_eq!(fs::read(root.join(PRESET)).unwrap(), before);
        let empty_root = dir.path().join("failed-first-install");
        assert!(
            commit_install(
                &payload,
                &empty_root,
                p.resolve(dir.path()).unwrap(),
                |_| bail!("Registration failed")
            )
            .is_err()
        );
        assert!(!empty_root.join(APPLICATION).exists());
        assert!(!empty_root.join(PRESET).exists());
    }
    #[cfg(windows)]
    #[test]
    fn native_shortcut_is_created_in_the_selected_directory() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Program/WorkFlow Media.exe");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"MZ-test").unwrap();
        let link = dir.path().join("Selected desktop/WorkFlow Media.lnk");
        platform::create_shortcut(&exe, &link).unwrap();
        let bytes = fs::read(link).unwrap();
        assert_eq!(&bytes[..4], &76u32.to_le_bytes());
        let target: Vec<u8> = exe
            .to_string_lossy()
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(bytes.windows(target.len()).any(|w| w == target));
    }
}
