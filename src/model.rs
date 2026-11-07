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
    pub start_on_add: bool,
    pub queue_paused: bool,
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub clipboard_mode: ClipboardMode,
    pub clipboard_sources: Vec<String>,
    pub service_preferences: BTreeMap<String, Preference>,
    pub hidden_columns: Vec<String>,
    pub language: String,
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
            start_on_add: true,
            queue_paused: false,
            start_with_windows: false,
            close_to_tray: true,
            clipboard_mode: ClipboardMode::Selected,
            clipboard_sources: vec!["YouTube".into(), "Facebook".into(), "FastShare".into()],
            service_preferences: BTreeMap::new(),
            hidden_columns: vec![],
            language: "cs".into(),
            automatic_updates: false,
        }
    }
}
impl Settings {
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
        if !self.cookies_file.is_empty() && !Path::new(&self.cookies_file).is_file() {
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
    let text = match text {
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
