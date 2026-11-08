//! Separate browser identities, encrypted snapshots, and bounded access-error fallback.
use crate::{
    model::{Browser, CookieMode, CookieProfile, Settings},
    platform, process, tools,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn key(profile: &CookieProfile) -> String {
    hex::encode(Sha256::digest(
        format!("{}\n{}", profile.browser.argument(), profile.path).as_bytes(),
    ))
}
pub fn argument(profile: &CookieProfile) -> Result<String> {
    profile.validate()?;
    Ok(if profile.path.is_empty() {
        profile.browser.argument().into()
    } else {
        format!("{}:{}", profile.browser.argument(), profile.path)
    })
}
fn roots() -> Vec<(Browser, PathBuf)> {
    let Some(base) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        let local = base.data_local_dir();
        let roaming = base.config_dir();
        for (b, p) in [
            (Browser::Chrome, "Google/Chrome/User Data"),
            (Browser::Brave, "BraveSoftware/Brave-Browser/User Data"),
            (Browser::Edge, "Microsoft/Edge/User Data"),
            (Browser::Chromium, "Chromium/User Data"),
            (Browser::Vivaldi, "Vivaldi/User Data"),
        ] {
            roots.push((b, local.join(p)));
        }
        for (b, p) in [
            (Browser::Opera, "Opera Software/Opera Stable"),
            (Browser::Opera, "Opera Software/Opera GX Stable"),
            (Browser::Firefox, "Mozilla/Firefox"),
        ] {
            roots.push((b, roaming.join(p)));
        }
        roots.push((
            Browser::Firefox,
            local.join("Packages/Mozilla.Firefox_n80bbvh6b1yt2/LocalCache/Roaming/Mozilla/Firefox"),
        ));
    }
    #[cfg(not(windows))]
    {
        for (b, p) in [
            (Browser::Chrome, "google-chrome"),
            (Browser::Brave, "BraveSoftware/Brave-Browser"),
            (Browser::Edge, "microsoft-edge"),
            (Browser::Chromium, "chromium"),
            (Browser::Vivaldi, "vivaldi"),
            (Browser::Opera, "opera"),
            (Browser::Firefox, "mozilla/firefox"),
        ] {
            roots.push((b, base.config_dir().join(p)));
        }
        for p in [
            ".mozilla/firefox",
            "snap/firefox/common/.mozilla/firefox",
            ".var/app/org.mozilla.firefox/.mozilla/firefox",
            ".var/app/org.mozilla.firefox/config/mozilla/firefox",
        ] {
            roots.push((Browser::Firefox, base.home_dir().join(p)));
        }
        for (b, p) in [
            (Browser::Chrome, "com.google.Chrome/config/google-chrome"),
            (
                Browser::Brave,
                "com.brave.Browser/config/BraveSoftware/Brave-Browser",
            ),
            (Browser::Chromium, "org.chromium.Chromium/config/chromium"),
            (Browser::Edge, "com.microsoft.Edge/config/microsoft-edge"),
        ] {
            roots.push((b, base.home_dir().join(".var/app").join(p)));
        }
    }
    roots
}
pub fn discover(browsers: &[Browser]) -> Vec<CookieProfile> {
    discover_in(&roots(), browsers)
}
fn small_text(path: &Path) -> Option<String> {
    (fs::metadata(path).ok()?.len() <= 8_000_000)
        .then(|| fs::read_to_string(path).ok())
        .flatten()
}
fn discover_in(roots: &[(Browser, PathBuf)], browsers: &[Browser]) -> Vec<CookieProfile> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (browser, root) in roots {
        if !browsers.contains(browser) {
            continue;
        }
        let names: serde_json::Value = small_text(&root.join("Local State"))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let mut candidates = vec![(root.clone(), String::new())];
        for parent in [root.clone(), root.join("Profiles")] {
            if let Ok(entries) = fs::read_dir(parent) {
                let mut entries: Vec<_> = entries
                    .flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .collect();
                entries.sort_by_key(|e| e.file_name());
                for entry in entries {
                    let label = names["profile"]["info_cache"]
                        [entry.file_name().to_string_lossy().as_ref()]["name"]
                        .as_str()
                        .unwrap_or("")
                        .to_owned();
                    candidates.push((entry.path(), label));
                }
            }
        }
        // profiles.ini may point outside Firefox's standard Profiles directory.
        if *browser == Browser::Firefox {
            if let Some(ini) = small_text(&root.join("profiles.ini")) {
                let mut section = String::new();
                let mut path = String::new();
                let mut label = String::new();
                let mut relative = true;
                for line in ini.lines().chain(std::iter::once("[end]")) {
                    let line = line.trim();
                    if line.starts_with('[') {
                        if section.starts_with("[Profile") && !path.is_empty() {
                            candidates.push((
                                if relative {
                                    root.join(&path)
                                } else {
                                    PathBuf::from(&path)
                                },
                                label.clone(),
                            ));
                        }
                        section = line.into();
                        path.clear();
                        label.clear();
                        relative = true;
                    } else if let Some((k, v)) = line.split_once('=') {
                        match k {
                            "Path" => path = v.into(),
                            "Name" => label = v.into(),
                            "IsRelative" => relative = v != "0",
                            _ => {}
                        }
                    }
                }
            }
        }
        for (path, label) in candidates {
            let exists = if *browser == Browser::Firefox {
                path.join("cookies.sqlite").is_file()
            } else {
                path.join("Network/Cookies").is_file() || path.join("Cookies").is_file()
            };
            if !exists {
                continue;
            }
            let Ok(path) = path.canonicalize() else {
                continue;
            };
            let path_string = path.to_string_lossy().into_owned();
            let label = if label.is_empty() {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                label
            };
            let profile = CookieProfile {
                browser: *browser,
                path: path_string,
                label: format!(
                    "{} · {}",
                    browser.label(),
                    label.chars().take(80).collect::<String>()
                ),
                enabled: true,
            };
            if profile.validate().is_ok() && seen.insert(key(&profile)) {
                result.push(profile);
            }
            if result.len() == 128 {
                return result;
            }
        }
    }
    result
}
pub fn profiles(settings: &Settings) -> Result<Vec<CookieProfile>> {
    let profiles = match settings.cookies_mode {
        CookieMode::Single => settings
            .cookies_browser
            .map(|b| CookieProfile {
                browser: b,
                path: String::new(),
                label: b.label().into(),
                enabled: true,
            })
            .into_iter()
            .collect(),
        CookieMode::Selected => settings
            .cookie_profiles
            .iter()
            .filter(|p| p.enabled)
            .cloned()
            .collect(),
        CookieMode::All => discover(&settings.cookies_browsers),
    };
    let mut seen = std::collections::HashSet::new();
    let profiles: Vec<_> = profiles
        .into_iter()
        .filter(|p| seen.insert(key(p)))
        .collect();
    if profiles.len() > 128 {
        bail!("Too many browser profiles / Příliš mnoho profilů prohlížeče");
    }
    for profile in &profiles {
        profile.validate()?;
    }
    if settings.uses_browser_cookies() && profiles.is_empty() {
        bail!(
            "No browser profiles found. Open a browser and sign in first / Nebyly nalezeny profily. Nejdříve otevřete prohlížeč a přihlaste se"
        );
    }
    Ok(profiles)
}

#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    captured: u64,
    #[serde(default)]
    last_refresh: u64,
    content: String,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn count(content: &str) -> usize {
    content
        .lines()
        .filter(|l| {
            (!l.starts_with('#') || l.starts_with("#HttpOnly_")) && l.split('\t').count() == 7
        })
        .count()
}
fn read_snapshot(path: &Path) -> Option<Snapshot> {
    let data = platform::load_secret(path).ok()?;
    if data.len() > 32_000_000 {
        return None;
    }
    serde_json::from_slice(&data).ok()
}
pub fn export(profile: &CookieProfile, downloader: &Path, cancel: &AtomicBool) -> Result<String> {
    let temporary = tempfile::tempdir()?;
    let info = temporary.path().join("local-info.json");
    fs::write(
        &info,
        br#"{"id":"cookie-refresh","title":"Cookies","url":"http://127.0.0.1:1/unused.mp4","ext":"mp4","extractor":"generic"}"#,
    )?;
    let output = temporary.path().join("cookies.txt");
    let mut command = platform::command(downloader);
    command
        .args([
            "--ignore-config",
            "--no-colors",
            "--skip-download",
            "--no-check-formats",
            "--no-cache-dir",
            "--cookies-from-browser",
        ])
        .arg(argument(profile)?)
        .arg("--cookies")
        .arg(&output)
        .arg("--load-info-json")
        .arg(&info);
    // Complete local metadata + skip-download export the jar on close without requesting the URL.
    process::run(&mut command, cancel, Some(Duration::from_secs(90)), |_| {})?;
    if fs::metadata(&output)?.len() > 24_000_000 {
        bail!("Cookie profile too large / Profil cookies je příliš velký");
    }
    let content = fs::read_to_string(output)?;
    if count(&content) == 0 {
        bail!(
            "No readable cookies; the profile is empty, locked or encrypted / Žádné čitelné cookies; profil je prázdný, zamčený nebo šifrovaný"
        );
    }
    Ok(content)
}
fn snapshot(
    profile: &CookieProfile,
    root: &Path,
    downloader: &Path,
    automatic: bool,
    force: bool,
    cancel: &AtomicBool,
) -> Result<Snapshot> {
    fs::create_dir_all(root)?;
    let id = key(profile);
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(format!("{id}.lock")))?;
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("@cancelled");
        }
        match fs2::FileExt::try_lock_exclusive(&lock) {
            Ok(()) => break,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
            {
                std::thread::sleep(Duration::from_millis(75))
            }
            Err(e) => return Err(e.into()),
        }
    }
    let path = root.join(format!("{id}.session"));
    let cached = read_snapshot(&path);
    if !force {
        if let Some(s) = &cached {
            let last_refresh = if s.last_refresh == 0 {
                s.captured
            } else {
                s.last_refresh
            };
            if !automatic || now().checked_sub(last_refresh).is_some_and(|age| age < 60) {
                return Ok(s.clone());
            }
        }
    }
    let content = match export(profile, downloader, cancel) {
        Ok(content) => content,
        Err(_e) if !force && !cancel.load(Ordering::Relaxed) && cached.is_some() => {
            let mut cached = cached.unwrap();
            cached.last_refresh = now();
            // Avoid repeating a failed browser read for every concurrent download.
            let _ = platform::save_secret(&path, &serde_json::to_vec(&cached)?);
            return Ok(cached);
        }
        Err(e) => return Err(e),
    };
    let snapshot = Snapshot {
        captured: now(),
        last_refresh: now(),
        content,
    };
    platform::save_secret(&path, &serde_json::to_vec(&snapshot)?).context(
        "Cannot securely store browser cookies / Nelze bezpečně uložit cookies prohlížeče",
    )?;
    Ok(snapshot)
}
fn cache_root() -> PathBuf {
    platform::app_data().join("cookie-profiles")
}

pub struct RefreshReport {
    pub profiles: Vec<CookieProfile>,
    pub results: Vec<(String, Result<usize, String>)>,
}
pub fn refresh(settings: &Settings, cancel: &AtomicBool) -> Result<RefreshReport> {
    let profiles = profiles(settings)?;
    let tools = tools::get()?;
    let mut results = Vec::new();
    for profile in &profiles {
        if cancel.load(Ordering::Relaxed) {
            bail!("@cancelled");
        }
        let result = snapshot(
            profile,
            &cache_root(),
            &tools.downloader,
            true,
            true,
            cancel,
        )
        .map(|s| count(&s.content))
        .map_err(|e| format!("{e:#}"));
        results.push((key(profile), result));
    }
    Ok(RefreshReport { profiles, results })
}
pub fn forget() -> Result<()> {
    let root = cache_root();
    // Linux Secret Service entries also have a lock file as their on-disk index.
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if id.len() == 64
                && id.bytes().all(|b| b.is_ascii_hexdigit())
                && path.extension().is_some_and(|e| e == "lock")
            {
                let file = fs::OpenOptions::new().read(true).write(true).open(&path)?;
                fs2::FileExt::try_lock_exclusive(&file).context("Cookies are being refreshed; try again shortly / Cookies se právě obnovují; zkuste to za chvíli")?;
                platform::delete_secret(&root.join(format!("{id}.session")))?;
            }
        }
    }
    Ok(())
}
pub fn is_access_error(error: &anyhow::Error) -> bool {
    // Local I/O failures must not be retried with another identity.
    if error
        .chain()
        .any(|e| e.downcast_ref::<std::io::Error>().is_some())
    {
        return false;
    }
    let text = format!("{error:#}").to_ascii_lowercase();
    // Match status messages, not digits inside a video ID, URL or filename.
    let status = |code: &str| {
        [
            "http error ",
            "http status ",
            "status code ",
            "status code: ",
            "http status client error (",
        ]
        .iter()
        .any(|prefix| {
            let needle = format!("{prefix}{code}");
            text.match_indices(&needle).any(|(index, _)| {
                !text[index + needle.len()..].starts_with(|c: char| c.is_ascii_digit())
            })
        })
    };
    if status("429") {
        return false;
    }
    if [
        "too many requests",
        "timed out",
        "connection refused",
        "connection reset",
        "connection aborted",
        "unable to connect",
        "failed to resolve",
        "network is unreachable",
        "no space",
        "permission denied",
        "access is denied",
        "unable to open for writing",
        "postprocessing:",
        "ffmpeg not found",
        "ffprobe not found",
        "no such file or directory",
        "@cancelled",
    ]
    .iter()
    .any(|s| text.contains(s))
    {
        return false;
    }
    if status("401") || status("403") {
        return true;
    }
    [
        "login required",
        "login-required",
        "logged in",
        "log in",
        "sign in",
        "private video",
        "private media",
        "private content",
        "private account",
        "video is private",
        "video unavailable",
        "video is unavailable",
        "content is not available",
        "content isn't available",
        "video is not available",
        "video not found",
        "members-only",
        "members only",
        "authentication",
        "access denied",
        "do not have permission",
        "authorized",
        "cookies are no longer valid",
        "cookies are not valid",
        "fresh cookies",
    ]
    .iter()
    .any(|s| text.contains(s))
}
/// Retry only access failures; disk, network, processing errors and cancellation terminate the job.
fn attempts<T>(
    profiles: &[CookieProfile],
    cancel: &AtomicBool,
    mut attempt: impl FnMut(usize, &CookieProfile) -> Result<T>,
    mut retry: impl FnMut(&anyhow::Error) -> bool,
) -> Result<T> {
    let mut errors = Vec::new();
    for (index, profile) in profiles.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            bail!("@cancelled");
        }
        match attempt(index, profile) {
            Ok(value) => return Ok(value),
            Err(e) if !cancel.load(Ordering::Relaxed) && retry(&e) => {
                errors.push(format!("{}: {}", profile.label, e))
            }
            Err(e) => return Err(e),
        }
    }
    bail!(
        "No selected profile could access this media / Žádný vybraný profil nezískal přístup k médiu:\n{}",
        errors.join("\n")
    )
}
pub fn with_auth<T>(
    settings: &Settings,
    cancel: &AtomicBool,
    progress: impl FnMut(usize, usize, &str),
    mut run: impl FnMut(&Settings, &str) -> Result<T>,
) -> Result<T> {
    if !settings.uses_browser_cookies() {
        if !settings.cookies_file.is_empty() {
            // Both inspection and downloading may update a jar; preserve the user's export.
            let file = tempfile::NamedTempFile::new()?;
            fs::copy(&settings.cookies_file, file.path())?;
            let mut configured = settings.clone();
            configured.cookies_file = file.path().to_string_lossy().into_owned();
            return run(&configured, "default");
        }
        return run(settings, "default");
    }
    let tools = tools::get()?;
    with_auth_at(
        settings,
        cancel,
        &cache_root(),
        &tools.downloader,
        progress,
        run,
    )
}
fn with_auth_at<T>(
    settings: &Settings,
    cancel: &AtomicBool,
    root: &Path,
    downloader: &Path,
    mut progress: impl FnMut(usize, usize, &str),
    mut run: impl FnMut(&Settings, &str) -> Result<T>,
) -> Result<T> {
    let profiles = profiles(settings)?;
    let prepared = std::cell::Cell::new(false);
    attempts(
        &profiles,
        cancel,
        |index, profile| {
            prepared.set(false);
            progress(index + 1, profiles.len(), &profile.label);
            let snapshot = snapshot(
                profile,
                root,
                downloader,
                settings.cookies_auto_refresh,
                false,
                cancel,
            )?;
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(snapshot.content.as_bytes())?;
            file.flush()?;
            let mut configured = settings.clone();
            configured.cookies_mode = CookieMode::Single;
            configured.cookies_browser = None;
            configured.cookies_file = file.path().to_string_lossy().into_owned();
            prepared.set(true);
            run(&configured, &key(profile))
        },
        |e| !prepared.get() || is_access_error(e),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(browser: Browser, path: PathBuf) -> CookieProfile {
        CookieProfile {
            browser,
            path: path.display().to_string(),
            label: "Test account".into(),
            enabled: true,
        }
    }
    #[test]
    fn discovers_six_chrome_two_brave_profiles_without_reading_cookie_contents() {
        let root = tempfile::tempdir().unwrap();
        let mut roots = Vec::new();
        for (browser, n) in [(Browser::Chrome, 6), (Browser::Brave, 2)] {
            let path = root.path().join(browser.argument());
            for i in 0..n {
                let db = path.join(format!("Profile {i}/Network"));
                fs::create_dir_all(&db).unwrap();
                fs::write(db.join("Cookies"), b"unreadable database fixture").unwrap();
            }
            roots.push((browser, path));
        }
        assert_eq!(discover_in(&roots, &Browser::ALL).len(), 8);
        assert_eq!(discover_in(&roots, &[Browser::Brave]).len(), 2);
        let first = discover_in(&roots, &Browser::ALL)[0].clone();
        assert!(argument(&first).unwrap().starts_with("chrome:"));
        assert!(Path::new(&first.path).is_absolute());
    }
    #[test]
    fn firefox_custom_profiles_are_discovered_and_deduplicated() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("Profiles/first");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("cookies.sqlite"), b"test").unwrap();
        fs::write(
            root.path().join("profiles.ini"),
            "[Profile0]\nName=First\nIsRelative=1\nPath=Profiles/first\n",
        )
        .unwrap();
        assert_eq!(
            discover_in(&[(Browser::Firefox, root.path().into())], &Browser::ALL).len(),
            1
        );
    }
    #[test]
    fn separate_accounts_try_in_order_and_stop_at_success_or_non_auth_failure() {
        let root = tempfile::tempdir().unwrap();
        let profiles: Vec<_> = (0..8)
            .map(|n| {
                profile(
                    if n < 6 {
                        Browser::Chrome
                    } else {
                        Browser::Brave
                    },
                    root.path().join(format!("Profile {n}")),
                )
            })
            .collect();
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let result = attempts(
            &profiles,
            &cancel,
            |i, p| {
                seen.push(key(p));
                if i == 7 {
                    Ok(i)
                } else {
                    bail!("HTTP Error 403: private media")
                }
            },
            is_access_error,
        )
        .unwrap();
        assert_eq!(result, 7);
        assert_eq!(seen.len(), 8);
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 8);
        let mut n = 0;
        assert!(
            attempts(
                &profiles,
                &cancel,
                |_, _| {
                    n += 1;
                    Err::<(), _>(anyhow::anyhow!("HTTP Error 429"))
                },
                is_access_error
            )
            .is_err()
        );
        assert_eq!(n, 1);
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            attempts(&profiles, &cancel, |_, _| Ok(()), is_access_error)
                .unwrap_err()
                .to_string(),
            "@cancelled"
        );
    }
    #[test]
    fn profile_paths_cannot_inject_a_container_or_command() {
        let mut p = profile(
            Browser::Chrome,
            std::env::temp_dir().join("account with spaces"),
        );
        assert!(argument(&p).unwrap().contains("account with spaces"));
        p.path.push_str("::container");
        assert!(argument(&p).is_err());
        p.path = "--exec command".into();
        assert!(argument(&p).is_err());
    }

    #[test]
    fn selected_profiles_keep_order_skip_disabled_and_deduplicate() {
        let root = tempfile::tempdir().unwrap();
        let first = profile(Browser::Brave, root.path().join("brave"));
        let second = profile(Browser::Chrome, root.path().join("chrome"));
        let mut disabled = profile(Browser::Firefox, root.path().join("firefox"));
        disabled.enabled = false;
        let s = Settings {
            cookies_mode: CookieMode::Selected,
            cookie_profiles: vec![first.clone(), disabled, second.clone(), first.clone()],
            ..Settings::default()
        };
        assert_eq!(profiles(&s).unwrap(), vec![first, second]);
    }
    #[test]
    fn fallback_distinguishes_access_failures_from_local_errors_and_video_ids() {
        for message in [
            "ERROR: [youtube] clip429: HTTP Error 403: Forbidden",
            "ERROR: [instagram] connection-story: This content is not available, login required",
            "HTTP status client error (401 Unauthorized)",
            "Join to access this members-only video",
            "You do not have permission to view this private video",
        ] {
            assert!(is_access_error(&anyhow::anyhow!(message)), "{message}");
        }
        for message in [
            "ERROR: ffmpeg not found",
            "ERROR: Postprocessing: conversion failed for private-video-403.mp4",
            "ERROR: Unable to open for writing: access is denied",
            "ERROR: HTTP Error 429: Too Many Requests",
            "ERROR: connection reset while requesting https://example.com/private/403",
            "ERROR: clip403-cookie failed: Unsupported URL",
            "ERROR: HTTP Error 4030",
        ] {
            assert!(!is_access_error(&anyhow::anyhow!(message)), "{message}");
        }
        let local = anyhow::Error::from(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Access denied",
        ));
        assert!(!is_access_error(&local));
    }
    #[test]
    fn manual_cookie_exports_are_unchanged_and_temporary_copies_are_removed() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("cookies.txt");
        fs::write(&original, "original cookie jar").unwrap();
        let settings = Settings {
            cookies_file: original.display().to_string(),
            ..Settings::default()
        };
        for fail in [false, true] {
            let mut temporary = PathBuf::new();
            let result = with_auth(
                &settings,
                &AtomicBool::new(false),
                |_, _, _| {},
                |s, id| {
                    assert_eq!(id, "default");
                    temporary = PathBuf::from(&s.cookies_file);
                    assert_ne!(temporary, original);
                    assert_eq!(fs::read_to_string(&temporary)?, "original cookie jar");
                    fs::write(&temporary, "updated by media tool")?;
                    if fail {
                        bail!("HTTP Error 403")
                    } else {
                        Ok(())
                    }
                },
            );
            assert_eq!(result.is_err(), fail);
            assert!(!temporary.exists());
            assert_eq!(
                fs::read_to_string(&original).unwrap(),
                "original cookie jar"
            );
        }
    }
    /// Full authentication path: the local server only serves media to the eighth account.
    #[cfg(windows)]
    #[test]
    #[ignore = "requires bundled tools and WFM_TEST_MEDIA"]
    fn real_http_download_uses_eighth_profile_and_preserves_manual_cookie_export() {
        use std::{
            io::{BufRead, BufReader},
            net::TcpListener,
            sync::{Arc, Mutex},
        };
        let media = Arc::new(fs::read(std::env::var_os("WFM_TEST_MEDIA").unwrap()).unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let (stop, observed, media) = (stop.clone(), observed.clone(), media.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut first = String::new();
                    if reader.read_line(&mut first).is_err() {
                        continue;
                    }
                    let mut cookie = String::new();
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some(value) = line.to_ascii_lowercase().strip_prefix("cookie:") {
                            cookie = value.trim().into();
                        }
                    }
                    let identity = cookie
                        .split(';')
                        .map(str::trim)
                        .find(|s| s.starts_with("sessionid="))
                        .unwrap_or("");
                    observed.lock().unwrap().push(identity.to_owned());
                    if identity != "sessionid=account_7" {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                    } else {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nSet-Cookie: server_cookie=updated; Path=/\r\nConnection: close\r\n\r\n",
                            media.len()
                        );
                        if !first.starts_with("HEAD ") {
                            let _ = stream.write_all(&media);
                        }
                    }
                }
            })
        };
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let mut selected = Vec::new();
        for n in 0..8 {
            let p = profile(
                if n < 6 {
                    Browser::Chrome
                } else {
                    Browser::Brave
                },
                root.path().join(format!("Profile {n}")),
            );
            let snapshot = Snapshot {
                captured: now(),
                last_refresh: now(),
                content: format!(
                    "# Netscape HTTP Cookie File\n127.0.0.1\tFALSE\t/\tFALSE\t0\tsessionid\taccount_{n}\n"
                ),
            };
            platform::save_secret(
                &cache.join(format!("{}.session", key(&p))),
                &serde_json::to_vec(&snapshot).unwrap(),
            )
            .unwrap();
            selected.push(p);
        }
        let settings = Settings {
            cookies_mode: CookieMode::Selected,
            cookies_auto_refresh: false,
            cookie_profiles: selected,
            ..Settings::default()
        };
        let cancel = AtomicBool::new(false);
        let tools = tools::get().unwrap();
        let job = crate::model::Job {
            url: format!("http://{address}/private-fixture.mp4"),
            directory: root.path().display().to_string(),
            ..Default::default()
        };
        let result = with_auth_at(
            &settings,
            &cancel,
            &cache,
            &tools.downloader,
            |_, _, _| {},
            |s, id| crate::media::download_one(&job, s, id, &cancel, &|_| {}),
        );
        // Also exercise metadata inspection: yt-dlp receives Set-Cookie but must not rewrite the user's export.
        let cookie_path = root.path().join("manual.txt");
        let original =
            "# Netscape HTTP Cookie File\n127.0.0.1\tFALSE\t/\tFALSE\t0\tsessionid\taccount_7\n";
        fs::write(&cookie_path, original).unwrap();
        let manual = Settings {
            cookies_file: cookie_path.display().to_string(),
            ..Settings::default()
        };
        let metadata = crate::media::inspect(&job.url, &manual, &cancel);
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        let output = result.unwrap();
        assert_eq!(
            Sha256::digest(fs::read(&output).unwrap()),
            Sha256::digest(media.as_slice())
        );
        assert!(metadata.is_ok(), "{metadata:?}");
        assert_eq!(fs::read_to_string(cookie_path).unwrap(), original);
        let mut order = observed.lock().unwrap().clone();
        order.dedup();
        assert_eq!(
            order,
            (0..8)
                .map(|n| format!("sessionid=account_{n}"))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            fs::read_dir(root.path())
                .unwrap()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "mp4"))
                .count(),
            1
        );
    }

    /// Synthetic databases only; see scripts/create-cookie-fixtures.py. Never uses personal profiles.
    #[cfg(windows)]
    #[test]
    #[ignore = "requires WFM_COOKIE_FIXTURES and WFM_COOKIE_DOWNLOADER"]
    fn real_cookie_exports_cache_refresh_and_eight_account_fallback() {
        let fixtures = PathBuf::from(std::env::var_os("WFM_COOKIE_FIXTURES").unwrap());
        let downloader = PathBuf::from(std::env::var_os("WFM_COOKIE_DOWNLOADER").unwrap());
        let ps = discover_in(
            &[
                (Browser::Chrome, fixtures.join("chrome")),
                (Browser::Brave, fixtures.join("brave")),
            ],
            &Browser::ALL,
        );
        assert_eq!(ps.len(), 8);
        let cache = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let s = Settings {
            cookies_mode: CookieMode::Selected,
            cookie_profiles: ps.clone(),
            ..Settings::default()
        };
        let mut paths = Vec::new();
        let mut attempted = Vec::new();
        let result = with_auth_at(
            &s,
            &cancel,
            cache.path(),
            &downloader,
            |i, total, _| {
                assert_eq!(total, 8);
                attempted.push(i);
            },
            |configured, identity| {
                assert!(!configured.uses_browser_cookies());
                let content = fs::read_to_string(&configured.cookies_file)?;
                assert_eq!(count(&content), 1);
                paths.push(configured.cookies_file.clone());
                let index = ps.iter().position(|p| key(p) == identity).unwrap();
                let browser = if index < 6 { "chrome" } else { "brave" };
                let n = if index < 6 { index } else { index - 6 };
                assert!(content.contains(&format!("SYNTHETIC_{browser}_{n}")));
                if index == 7 {
                    Ok(identity.to_owned())
                } else {
                    bail!("HTTP Error 403: private media")
                }
            },
        )
        .unwrap();
        assert_eq!(result, key(&ps[7]));
        assert_eq!(attempted, (1..=8).collect::<Vec<_>>());
        assert!(paths.iter().all(|p| !Path::new(p).exists()));
        for p in &ps {
            let path = cache.path().join(format!("{}.session", key(p)));
            let encrypted = fs::read(&path).unwrap();
            assert!(
                !encrypted
                    .windows(b"SYNTHETIC".len())
                    .any(|w| w == b"SYNTHETIC")
            );
            assert_eq!(count(&read_snapshot(&path).unwrap().content), 1);
        }
        let p = &ps[0];
        let path = cache.path().join(format!("{}.session", key(p)));
        let unavailable = cache.path().join("missing-downloader.exe");
        // Disabled automatic refresh and a fresh snapshot do not require a running browser/tool.
        assert!(snapshot(p, cache.path(), &unavailable, false, false, &cancel).is_ok());
        assert!(snapshot(p, cache.path(), &unavailable, true, false, &cancel).is_ok());
        let mut old = read_snapshot(&path).unwrap();
        old.captured = 0;
        old.last_refresh = 0;
        old.content = old.content.replace("SYNTHETIC_chrome_0", "STALE_SYNTHETIC");
        platform::save_secret(&path, &serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(
            snapshot(p, cache.path(), &unavailable, true, false, &cancel)
                .unwrap()
                .content
                .contains("STALE_SYNTHETIC")
        );
        assert!(snapshot(p, cache.path(), &unavailable, true, true, &cancel).is_err());
        assert!(
            read_snapshot(&path)
                .unwrap()
                .content
                .contains("STALE_SYNTHETIC")
        );
        // The failed automatic refresh is throttled too. Manual refresh bypasses that interval.
        let fresh = snapshot(p, cache.path(), &downloader, true, true, &cancel).unwrap();
        assert!(fresh.content.contains("SYNTHETIC_chrome_0"));
        assert!(fresh.captured > 0);
        let mut future = fresh.clone();
        future.captured = now() + 3600;
        future.last_refresh = future.captured;
        platform::save_secret(&path, &serde_json::to_vec(&future).unwrap()).unwrap();
        let clock_recovered = snapshot(p, cache.path(), &downloader, true, false, &cancel).unwrap();
        assert!(clock_recovered.captured <= now());
        assert!(clock_recovered.last_refresh <= now());
        // An unreadable profile is skipped; a disabled one is never tried.
        let mut bad = profile(Browser::Chrome, cache.path().join("missing-profile"));
        let mut selected = s.clone();
        selected.cookie_profiles = vec![bad.clone(), ps[7].clone()];
        let mut n = 0;
        with_auth_at(
            &selected,
            &cancel,
            cache.path(),
            &downloader,
            |_, _, _| {},
            |_, id| {
                n += 1;
                assert_eq!(id, key(&ps[7]));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        bad.enabled = false;
        selected.cookie_profiles = vec![bad, ps[7].clone()];
        let mut progress = 0;
        with_auth_at(
            &selected,
            &cancel,
            cache.path(),
            &downloader,
            |_, total, _| {
                progress += 1;
                assert_eq!(total, 1);
            },
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(progress, 1);
        cancel.store(true, Ordering::Relaxed);
        assert!(
            with_auth_at(
                &s,
                &cancel,
                cache.path(),
                &downloader,
                |_, _, _| {},
                |_, _| Ok(())
            )
            .is_err()
        );
    }
}
