use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, Local};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use url::Url;

pub const SOURCES: [&str; 7] = [
    "YouTube",
    "Facebook",
    "FastShare",
    "Instagram",
    "TikTok",
    "Vimeo",
    "Web",
];
pub const QUALITIES: [i32; 9] = [0, 4320, 2160, 1440, 1080, 720, 480, 360, 240];
pub const MAX_PARALLEL: usize = 16;
/// An application-owned theme. System colors never select or modify it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}
impl Theme {
    pub fn label(self, lang: &str) -> &'static str {
        match self {
            Self::Dark => tr(lang, "Tmavý", "Dark"),
            Self::Light => tr(lang, "Světlý", "Light"),
        }
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CookieMode {
    #[default]
    Single,
    Selected,
    All,
}
pub fn default_cookie_browsers() -> Vec<Browser> {
    Browser::ALL.to_vec()
}
pub fn default_cookie_refresh() -> bool {
    true
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CookieProfile {
    pub browser: Browser,
    pub path: String,
    pub label: String,
    pub enabled: bool,
}
impl CookieProfile {
    pub fn validate(&self) -> Result<()> {
        if (!self.path.is_empty() && !Path::new(&self.path).is_absolute())
            || self.path.contains(['\0', '\r', '\n'])
            || self.path.contains("::")
            || self.path.len() > 32768
            || self.label.len() > 512
        {
            bail!("Invalid browser profile / Neplatný profil prohlížeče");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Browser {
    Chrome,
    Brave,
    Firefox,
    Edge,
    Chromium,
    Opera,
    Vivaldi,
}
impl Browser {
    pub const ALL: [Self; 7] = [
        Self::Chrome,
        Self::Brave,
        Self::Firefox,
        Self::Edge,
        Self::Chromium,
        Self::Opera,
        Self::Vivaldi,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Chrome => "Google Chrome",
            Self::Brave => "Brave",
            Self::Firefox => "Mozilla Firefox",
            Self::Edge => "Microsoft Edge",
            Self::Chromium => "Chromium",
            Self::Opera => "Opera",
            Self::Vivaldi => "Vivaldi",
        }
    }
    pub fn argument(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Brave => "brave",
            Self::Firefox => "firefox",
            Self::Edge => "edge",
            Self::Chromium => "chromium",
            Self::Opera => "opera",
            Self::Vivaldi => "vivaldi",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|b| b.argument() == value)
            .context("Unknown browser / Neznámý prohlížeč")
    }
}
pub fn tr<'a>(lang: &str, cs: &'a str, en: &'a str) -> &'a str {
    if lang == "cs" { cs } else { en }
}
pub fn now() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobState {
    #[default]
    Queued,
    Connecting,
    Downloading,
    Processing,
    Paused,
    Completed,
    Failed,
    Cancelled,
}
impl JobState {
    pub fn active(self) -> bool {
        matches!(
            self,
            Self::Connecting | Self::Downloading | Self::Processing
        )
    }
    pub fn label(self, lang: &str) -> &'static str {
        match self {
            Self::Queued => tr(lang, "Ve frontě", "Queued"),
            Self::Connecting => tr(lang, "Připojování", "Connecting"),
            Self::Downloading => tr(lang, "Stahování", "Downloading"),
            Self::Processing => tr(lang, "Zpracování", "Processing"),
            Self::Paused => tr(lang, "Pozastaveno", "Paused"),
            Self::Completed => tr(lang, "Dokončeno", "Completed"),
            Self::Failed => tr(lang, "Vyžaduje pozornost", "Needs attention"),
            Self::Cancelled => tr(lang, "Zrušeno", "Cancelled"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Job {
    pub id: String,
    pub url: String,
    pub title: String,
    pub source: String,
    pub format: String,
    pub quality: i32,
    pub directory: String,
    pub file_path: String,
    pub validator: String,
    pub state: JobState,
    pub downloaded: u64,
    pub total: u64,
    pub speed: f64,
    pub percent: f64,
    pub message: String,
    pub log: String,
    pub created: DateTime<FixedOffset>,
    pub finished: Option<DateTime<FixedOffset>>,
    pub hidden_from_queue: bool,
    pub removed_from_library: bool,
}
impl Default for Job {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            url: String::new(),
            title: String::new(),
            source: "Web".into(),
            format: "MP4".into(),
            quality: 0,
            directory: String::new(),
            file_path: String::new(),
            validator: String::new(),
            state: JobState::Queued,
            downloaded: 0,
            total: 0,
            speed: 0.,
            percent: 0.,
            message: String::new(),
            log: String::new(),
            created: now(),
            finished: None,
            hidden_from_queue: false,
            removed_from_library: false,
        }
    }
}
impl Job {
    pub fn in_library(&self) -> bool {
        self.state == JobState::Completed && !self.removed_from_library
    }
}

#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClipboardMode {
    Off,
    #[default]
    Selected,
    All,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Preference {
    pub format: String,
    pub quality: i32,
}
impl Default for Preference {
    fn default() -> Self {
        Self {
            format: String::new(),
            quality: -1,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Settings {
    pub download_directory: String,
    pub parallel: usize,
    pub format: String,
    pub quality: i32,
    pub cookies_file: String,
    pub cookies_browser: Option<Browser>,
    pub cookies_mode: CookieMode,
    pub cookies_auto_refresh: bool,
    pub cookies_browsers: Vec<Browser>,
    pub cookie_profiles: Vec<CookieProfile>,
    pub start_on_add: bool,
    pub queue_paused: bool,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub clipboard_mode: ClipboardMode,
    pub clipboard_sources: Vec<String>,
    pub service_preferences: BTreeMap<String, Preference>,
    pub hidden_columns: Vec<String>,
    pub language: String,
    pub theme: Theme,
    pub automatic_updates: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            download_directory: directories::UserDirs::new()
                .map(|d| {
                    d.download_dir()
                        .unwrap_or(d.home_dir())
                        .join("WorkFlow Media")
                })
                .unwrap_or_else(|| PathBuf::from("downloads"))
                .to_string_lossy()
                .into(),
            parallel: 2,
            format: "MP4".into(),
            quality: 0,
            cookies_file: String::new(),
            cookies_browser: None,
            cookies_mode: CookieMode::Single,
            cookies_auto_refresh: true,
            cookies_browsers: default_cookie_browsers(),
            cookie_profiles: Vec::new(),
            start_on_add: true,
            queue_paused: false,
            start_with_windows: false,
            close_to_tray: true,
            clipboard_mode: ClipboardMode::Selected,
            clipboard_sources: vec!["YouTube".into(), "Facebook".into(), "FastShare".into()],
            service_preferences: BTreeMap::new(),
            hidden_columns: vec![],
            language: "cs".into(),
            theme: Theme::Dark,
            automatic_updates: false,
        }
    }
}
impl Settings {
    pub fn uses_browser_cookies(&self) -> bool {
        self.cookies_mode != CookieMode::Single || self.cookies_browser.is_some()
    }
    pub fn preference(&self, source: &str) -> Preference {
        if source == "FastShare" {
            return Preference {
                format: "Originál".into(),
                quality: 0,
            };
        }
        let p = self
            .service_preferences
            .get(source)
            .cloned()
            .unwrap_or_default();
        Preference {
            format: if p.format.is_empty() {
                self.format.clone()
            } else {
                p.format
            },
            quality: if p.quality < 0 {
                self.quality
            } else {
                p.quality
            },
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.language.as_str(), "cs" | "en") {
            bail!("Invalid language / Neplatný jazyk");
        }
        if !["MP4", "MKV", "MP3"].contains(&self.format.as_str())
            || !QUALITIES.contains(&self.quality)
        {
            bail!("Invalid format or quality / Neplatný formát nebo kvalita");
        }
        if !(1..=MAX_PARALLEL).contains(&self.parallel) {
            bail!("Parallel downloads: 1–{MAX_PARALLEL} / Souběžné úlohy: 1–{MAX_PARALLEL}");
        }
        if self.clipboard_mode == ClipboardMode::Selected && self.clipboard_sources.is_empty() {
            bail!("Select at least one service / Vyberte alespoň jednu službu");
        }
        if self
            .clipboard_sources
            .iter()
            .any(|s| !SOURCES.contains(&s.as_str()))
        {
            bail!("Unknown service / Neznámá služba");
        }
        for p in self.service_preferences.values() {
            if (!p.format.is_empty() && !matches!(p.format.as_str(), "MP4" | "MKV" | "MP3"))
                || (p.quality != -1 && !QUALITIES.contains(&p.quality))
            {
                bail!("Invalid service preset / Neplatná předvolba služby");
            }
        }
        if !Path::new(&self.download_directory).is_absolute() {
            bail!("Choose an absolute destination path / Vyberte úplnou cestu cílové složky");
        }
        if self.cookie_profiles.len() > 128 {
            bail!("Too many browser profiles / Příliš mnoho profilů prohlížeče");
        }
        for p in &self.cookie_profiles {
            p.validate()?;
        }
        if self.cookies_mode == CookieMode::All && self.cookies_browsers.is_empty() {
            bail!("Select a browser / Vyberte prohlížeč");
        }
        if self.cookies_mode == CookieMode::Selected
            && !self.cookie_profiles.iter().any(|p| p.enabled)
        {
            bail!("Select at least one profile / Vyberte alespoň jeden profil");
        }
        if !self.uses_browser_cookies()
            && !self.cookies_file.is_empty()
            && !Path::new(&self.cookies_file).is_file()
        {
            bail!("Cookies file does not exist / Soubor cookies neexistuje");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct State {
    pub version: u32,
    pub settings: Settings,
    pub jobs: Vec<Job>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            settings: Settings::default(),
            jobs: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Preset {
    pub version: u32,
    pub setup_completed: bool,
    #[serde(flatten)]
    pub settings: Settings,
}
impl Default for Preset {
    fn default() -> Self {
        Self {
            version: 1,
            setup_completed: false,
            settings: Settings::default(),
        }
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    if path.exists() {
        fs::copy(path, PathBuf::from(format!("{}.bak", path.display())))?;
    }
    crate::platform::replace_file(&temporary, path)?;
    Ok(())
}
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(value)?)
}
pub fn load_state(path: &Path) -> Result<State> {
    if !path.exists() {
        return Ok(State::default());
    }
    let read = |p: &Path| -> Result<State> {
        let s: State = serde_json::from_slice(&fs::read(p)?)?;
        if s.version != 1 {
            bail!("Unsupported data version / Nepodporovaná verze dat");
        }
        Ok(s)
    };
    let mut state=read(path).or_else(|_|read(&PathBuf::from(format!("{}.bak",path.display())))).context("Cannot read queue; original data preserved / Frontu nelze načíst; původní data zachována")?;
    state.settings.parallel = state.settings.parallel.clamp(1, MAX_PARALLEL);
    for job in &mut state.jobs {
        job.speed = 0.;
        if job.state.active() {
            job.state = JobState::Paused;
            job.message = "@interrupted".into();
        }
    }
    Ok(state)
}
pub fn parse_url(text: &str) -> Result<Url> {
    let u = Url::parse(text.trim()).context("Invalid link / Neplatný odkaz")?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        bail!(
            "Enter an HTTP(S) link without credentials / Vložte odkaz HTTP(S) bez přihlašovacích údajů"
        );
    }
    Ok(u)
}
pub fn is_fastshare(u: &Url) -> bool {
    let h = u.host_str().unwrap_or_default().to_ascii_lowercase();
    ["fastshare.cz", "fastshare.cloud"].iter().any(|domain| {
        if h == *domain {
            return true;
        }
        if let Some(prefix) = h.strip_suffix(&format!(".{domain}")) {
            prefix == "www"
                || prefix == "img"
                || prefix
                    .strip_prefix("data")
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        } else {
            false
        }
    })
}
pub fn source(u: &Url) -> &'static str {
    let h = u.host_str().unwrap_or_default();
    let host = |d: &str| h == d || h.ends_with(&format!(".{d}"));
    if is_fastshare(u) {
        "FastShare"
    } else if host("youtube.com") || host("youtu.be") {
        "YouTube"
    } else if host("facebook.com") || host("fb.watch") {
        "Facebook"
    } else if host("instagram.com") {
        "Instagram"
    } else if host("tiktok.com") {
        "TikTok"
    } else if host("vimeo.com") {
        "Vimeo"
    } else {
        "Web"
    }
}
pub fn file_id(u: &Url) -> Result<String> {
    let id = if u.path() == "/download.php" {
        u.query_pairs()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_default()
    } else {
        u.path()
            .trim_start_matches('/')
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    if id.is_empty() || id.len() > 18 || !id.bytes().all(|b| b.is_ascii_digit()) {
        bail!("FastShare link is missing a file ID / V odkazu FastShare chybí číslo souboru");
    }
    Ok(id)
}
pub fn canonical(u: &Url) -> Result<String> {
    if is_fastshare(u) {
        return Ok(format!("fastshare:{}", file_id(u)?));
    }
    if source(u) == "YouTube" {
        let id = if u.host_str() == Some("youtu.be") {
            u.path().trim_matches('/').to_owned()
        } else {
            u.query_pairs()
                .find(|(k, _)| k == "v")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default()
        };
        if !id.is_empty() {
            return Ok(format!("youtube:{id}"));
        }
    }
    let mut u = u.clone();
    u.set_fragment(None);
    Ok(u.to_string())
}
pub fn clipboard_links(text: &str, s: &Settings) -> Vec<String> {
    if s.clipboard_mode == ClipboardMode::Off || text.len() > 32768 {
        return vec![];
    }
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() || lines.len() > 100 {
        return vec![];
    }
    let parsed: Result<Vec<_>> = lines.iter().map(|l| parse_url(l)).collect();
    let Ok(urls) = parsed else {
        return vec![];
    };
    let mut seen = std::collections::HashSet::new();
    urls.into_iter()
        .filter(|u| {
            s.clipboard_mode == ClipboardMode::All
                || s.clipboard_sources.iter().any(|s| s == source(u))
        })
        .filter(|u| canonical(u).is_ok_and(|c| seen.insert(c)))
        .map(|u| u.to_string())
        .collect()
}
pub fn safe_name(text: &str) -> String {
    let mut s: String = text
        .chars()
        .map(|c| {
            if c < ' ' || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(150)
        .collect();
    s = s.trim_matches([' ', '.']).to_string();
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
    {
        s.insert(0, '_');
    }
    if s.is_empty() { "file.bin".into() } else { s }
}
pub fn bytes(mut n: f64) -> String {
    if !n.is_finite() || n <= 0. {
        return "0 B".into();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0;
    while n >= 1024. && i < 4 {
        n /= 1024.;
        i += 1;
    }
    if i == 0 {
        format!("{n:.0} {}", units[i])
    } else {
        format!("{n:.1} {}", units[i])
    }
}
pub fn message(lang: &str, text: &str) -> String {
    if let Some(profile) = text.strip_prefix("@cookie-profile:") {
        return format!("{} {profile}", tr(lang, "Zkouším profil", "Trying profile"));
    }
    let text = match text {
        "@without-cookies" => tr(lang, "Zkouším bez cookies…", "Trying without cookies…"),
        "@interrupted" => tr(
            lang,
            "Přerušeno zavřením aplikace. Pokračujte tlačítkem Obnovit.",
            "Interrupted by shutdown. Choose Resume to continue.",
        ),
        "@queued" => tr(lang, "Čeká ve frontě", "Waiting in queue"),
        "@paused" => tr(
            lang,
            "Pozastaveno · rozpracovaná data zůstávají",
            "Paused · partial files preserved",
        ),
        "@connecting" => tr(
            lang,
            "Zjišťuji dostupnost souboru…",
            "Checking file availability…",
        ),
        "@processing" => tr(
            lang,
            "Zpracovávám výsledný soubor…",
            "Processing the output file…",
        ),
        "@completed" => tr(lang, "Soubor byl uložen", "File saved"),
        "@cancelled" => tr(
            lang,
            "Zrušeno · rozpracovaná data lze obnovit",
            "Cancelled · partial files can be resumed",
        ),
        _ => text,
    };
    localized_error(lang, text)
}
/// Legacy engine errors retain both translations in logs; the UI shows one language.
pub fn localized_error(lang: &str, text: &str) -> String {
    if let Some(details) = text.strip_prefix("No selected profile could access this media / Žádný vybraný profil nezískal přístup k médiu:\n") {
        let details = details.lines().map(|line| {
            if let Some((label, error)) = line.split_once(": ") {
                format!("{label}: {}", localized_error(lang, error))
            } else { line.to_owned() }
        }).collect::<Vec<_>>().join("\n");
        return format!("{}\n{details}", tr(lang, "Žádný vybraný profil nezískal přístup k médiu:", "No selected profile could access this media:"));
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("could not copy") && lower.contains("cookie")
        || lower.contains("database is locked")
    {
        return tr(lang,
            "Prohlížeč zamyká cookies. Zavřete všechna jeho okna i procesy na pozadí a zkuste Obnovit. Případně v Nastavení zvolte Firefox nebo vlastní cookies.txt.",
            "The browser has locked its cookies. Close its windows and background processes, then choose Resume. Alternatively select Firefox or your own cookies.txt in Settings.").into();
    }
    if lower.contains("decrypt") || lower.contains("dpapi") || lower.contains("app-bound") {
        return tr(lang,
            "Systémová ochrana prohlížeče nepovolila načíst cookies. V Nastavení vyberte Firefox nebo vlastní export cookies.txt. Aplikaci spouštějte pod účtem, kterému patří profil prohlížeče.",
            "Browser encryption prevented cookie access. Select Firefox or your own cookies.txt export in Settings. Run the app under the account that owns the browser profile.").into();
    }
    if lower.contains("could not find") && (lower.contains("cookie") || lower.contains("browser")) {
        return tr(lang, "Profil vybraného prohlížeče nebyl nalezen. Nejdříve v něm otevřete požadovanou službu a přihlaste se, nebo v Nastavení vyberte jiný prohlížeč.",
            "The selected browser profile was not found. Open the service and sign in in that browser first, or select another browser in Settings.").into();
    }
    if lower.contains("sign in")
        || lower.contains("login required")
        || lower.contains("log in")
        || lower.contains("authentication required")
        || lower.contains("cookies-from-browser")
    {
        return tr(lang, "Služba vyžaduje přihlášení nebo ověření přístupu. V Nastavení zapněte cookies z prohlížeče, ve kterém jste přihlášeni, nebo vyberte vlastní cookies.txt. Poté zkuste Obnovit.",
            "The service requires sign-in or access verification. In Settings enable cookies from a browser where you are signed in, or select your own cookies.txt. Then try Resume.").into();
    }
    if text.contains("Instagram") && text.contains("bot-check") {
        return tr(lang,
            "Instagram požaduje ověření přístupu. V Nastavení vyberte aktuální vlastní cookies.txt z přihlášeného prohlížeče a zkuste úlohu obnovit.",
            "Instagram requires access verification. Select your own current cookies.txt from a signed-in browser in Settings, then try Resume.").into();
    }
    if text.contains("HTTP status client error (403 Forbidden)") || text.contains("HTTP Error 403")
    {
        return tr(lang,
            "Server odmítl přenos (HTTP 403). Zkuste úlohu obnovit. Pokud služba vyžaduje přihlášení, vyberte cookies z přihlášeného prohlížeče nebo vlastní cookies.txt v Nastavení.",
            "The server refused the transfer (HTTP 403). Try Resume. If the service requires sign-in, select browser cookies or your own cookies.txt in Settings.").into();
    }
    let Some((en, cs)) = text.split_once(" / ") else {
        return text.to_owned();
    };
    if !cs
        .chars()
        .any(|c| "áčďéěíňóřšťúůýžÁČĎÉĚÍŇÓŘŠŤÚŮÝŽ".contains(c))
    {
        return text.to_owned();
    }
    if lang == "cs" {
        cs.to_owned()
    } else if let Some((_, detail)) = cs.split_once(": ") {
        format!("{en}: {detail}")
    } else {
        en.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_preferences_are_backward_compatible_and_allowlisted() {
        let legacy: Settings = serde_json::from_str(r#"{"CookiesFile":"old-export.txt"}"#).unwrap();
        assert!(legacy.cookies_browser.is_none());
        assert_eq!(legacy.cookies_file, "old-export.txt");
        assert!(
            serde_json::from_str::<Settings>(r#"{"CookiesBrowser":"chrome:private-path"}"#)
                .is_err()
        );
        assert!(Browser::parse("chrome --exec anything").is_err());
        let mut s = Settings::default();
        s.cookies_browser = Some(Browser::Edge);
        s.cookies_file = "missing-inactive-file.txt".into();
        assert!(s.validate().is_ok());
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.cookies_browser, Some(Browser::Edge));
        for text in [
            "Could not copy Chrome cookie database",
            "Failed to decrypt with DPAPI",
            "Sign in to confirm",
            "could not find firefox cookies database",
        ] {
            assert_ne!(localized_error("cs", text), text);
            assert_ne!(localized_error("en", text), text);
        }
    }
    #[test]
    fn access_errors_explain_next_steps_in_both_languages() {
        let instagram = "media unavailable: bot-check — Instagram rejected the request (HTTP 403); retry with cookies";
        assert!(localized_error("cs", instagram).contains("Nastavení"));
        assert!(localized_error("en", instagram).contains("Settings"));
        let denied = "HTTP status client error (403 Forbidden) for url ([URL])";
        assert!(localized_error("cs", denied).contains("obnovit"));
        assert!(localized_error("en", denied).contains("Resume"));
        assert!(!localized_error("cs", denied).contains("[URL]"));
    }
    #[test]
    fn parallel_limit_survives_reload_and_rejects_invalid_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::default();
        state.settings.download_directory = dir.path().to_string_lossy().into_owned();
        for parallel in [1, 4, 8, 16] {
            state.settings.parallel = parallel;
            state.settings.validate().unwrap();
            save_json(&dir.path().join("queue.json"), &state).unwrap();
            let restored = load_state(&dir.path().join("queue.json")).unwrap();
            assert_eq!(restored.settings.parallel, parallel);
        }
        for invalid in [0, 17] {
            state.settings.parallel = invalid;
            assert!(state.settings.validate().is_err());
        }
    }
    #[test]
    fn url_boundaries() {
        assert!(!is_fastshare(
            &parse_url("https://fastshare.cz.evil.test/1").unwrap()
        ));
        assert!(!is_fastshare(
            &parse_url("https://evil.fastshare.cz/1").unwrap()
        ));
        assert!(is_fastshare(
            &parse_url("https://data12.fastshare.cloud/1").unwrap()
        ));
        assert!(parse_url("https://user:pass@example.com").is_err());
    }
    #[test]
    fn youtube_duplicates() {
        assert_eq!(
            canonical(&parse_url("https://youtu.be/abcd").unwrap()).unwrap(),
            canonical(&parse_url("https://www.youtube.com/watch?v=abcd&x=1").unwrap()).unwrap()
        );
    }
    #[test]
    fn clipboard_is_strict() {
        let s = Settings::default();
        assert!(clipboard_links("look at https://youtube.com/watch?v=x", &s).is_empty());
        assert_eq!(
            clipboard_links(
                "https://youtu.be/abcd\nhttps://youtube.com/watch?v=abcd",
                &s
            )
            .len(),
            1
        );
        assert!(clipboard_links("https://example.com/a", &s).is_empty());
    }
    #[test]
    fn legacy_schema() {
        let s:State=serde_json::from_str(r#"{"Version":1,"Settings":{"Format":"MP4"},"Jobs":[{"Id":"existing","State":"Completed","Title":"Save","Created":"2026-11-07T10:00:00+01:00"}]}"#).unwrap();
        assert!(s.jobs[0].in_library());
        assert_eq!(s.jobs[0].title, "Save");
        assert_eq!(s.settings.language, "cs");
    }
    #[test]
    fn names() {
        assert_eq!(safe_name("../CON.mp4"), "_CON.mp4");
        assert_eq!(safe_name("a:b?.mp4"), "a_b_.mp4");
    }
    #[test]
    fn presets() {
        let mut s = Settings::default();
        s.service_preferences.insert(
            "YouTube".into(),
            Preference {
                format: "MP3".into(),
                quality: 720,
            },
        );
        assert_eq!(s.preference("YouTube").quality, 720);
        assert_eq!(s.preference("FastShare").format, "Originál");
    }
}
