#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod cookies;
mod engine;
mod fastshare;
mod installer;
mod media;
mod model;
mod platform;
mod process;
mod style;
mod tools;
mod tray;
mod updater;
use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn main() {
    if let Err(e) = run() {
        let text = format!("{e:#}");
        eprintln!("{text}");
        if !std::env::args().any(|a| {
            matches!(
                a.as_str(),
                "--inspect-url"
                    | "--download-url"
                    | "--convert-media"
                    | "--sign-release"
                    | "--export-builder"
                    | "--make-installer"
                    | "--test-install-to"
            )
        }) {
            let _ = rfd::MessageDialog::new()
                .set_title("WorkFlow Media")
                .set_description(&text)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--authorize-setup") {
        return Ok(());
    }
    if args.iter().any(|a| a == "--version") {
        println!("WorkFlow Media {} · Rust", updater::VERSION);
        return Ok(());
    }
    if args.iter().any(|a| a == "--apply-update") {
        return updater::apply(&args);
    }
    if args.iter().any(|a| a == "--sign-release") {
        return sign_release(&args);
    }
    if args
        .iter()
        .any(|a| a == "--inspect-url" || a == "--download-url" || a == "--convert-media")
    {
        return media_command(&args);
    }
    let arg = |key: &str| {
        args.windows(2)
            .find(|w| w[0] == key)
            .map(|w| PathBuf::from(&w[1]))
    };
    let exe = std::env::current_exe()?;
    if args.iter().any(|a| a == "--export-builder") {
        return installer::export_builder(&arg("--output").context("Missing --output")?);
    }
    if args.iter().any(|a| a == "--make-installer") {
        if !cfg!(windows) {
            bail!("Build Windows installers on Windows");
        }
        let profile: installer::Profile =
            serde_json::from_slice(&fs::read(arg("--profile").context("Missing --profile")?)?)?;
        return installer::create_installer(
            &arg("--output").context("Missing --output")?,
            &profile,
        );
    }
    let package = installer::read_package(&exe)?.map(|(_, p)| p);
    let builder = cfg!(feature = "setup-builder")
        || args.iter().any(|a| a == "--setup-builder")
        || matches!(package, Some(installer::Package::Builder));
    if builder && !cfg!(windows) {
        bail!("The installer builder targets Windows; the main application also builds on Linux");
    }
    if !builder {
        if let Some(installer::Package::Installer(profile)) = package {
            if let Some(root) = arg("--test-install-to") {
                println!("{}", installer::test_install(&profile, &root)?.display());
                return Ok(());
            }
            return installer::run(profile);
        }
    }
    let folder = exe.parent().context("Executable folder")?;
    let preview = args.iter().any(|a| a == "--setup-preview");
    let isolated = builder || args.iter().any(|a| a == "--isolated") || arg("--data").is_some();
    let builder_temp = if builder {
        Some(tempfile::tempdir()?)
    } else {
        None
    };
    let data = builder_temp
        .as_ref()
        .map(|d| d.path().to_path_buf())
        .or_else(|| arg("--data"))
        .unwrap_or_else(platform::app_data);
    if !data.is_absolute() {
        bail!("--data must be an absolute path");
    }
    fs::create_dir_all(&data)?;
    // Read before acquiring the lock so a second launch during startup is not lost.
    let initial_signal = fs::read_to_string(data.join("show.signal")).ok();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data.join("instance.lock"))?;
    if let Err(error) = fs2::FileExt::try_lock_exclusive(&lock) {
        if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
            fs::write(data.join("show.signal"), uuid::Uuid::new_v4().to_string())?;
            return Ok(());
        }
        return Err(error).context("Cannot lock application instance");
    }
    let preset_path = if builder {
        data.join("builder.preset.json")
    } else {
        arg("--preset").unwrap_or_else(|| folder.join("WorkFlow Media.preset.json"))
    };
    // A separate application data directory protects the previous .NET version.
    if !isolated && !preview && !data.join("queue.json").exists() {
        if let Some(legacy) = platform::legacy_data() {
            if legacy.join("queue.json").is_file() {
                let mut state = model::load_state(&legacy.join("queue.json"))?;
                for j in &mut state.jobs {
                    if j.state == model::JobState::Queued {
                        j.state = model::JobState::Paused;
                        j.message = "@interrupted".into();
                    }
                }
                model::save_json(&data.join("queue.json"), &state)?;
                if legacy.join("fastshare.session").is_file() {
                    fs::copy(
                        legacy.join("fastshare.session"),
                        data.join("fastshare.session"),
                    )?;
                }
            }
        }
    }
    let preset = if preset_path.is_file() {
        Some(read_preset(
            &preset_path,
            &model::load_state(&data.join("queue.json"))?.settings,
        )?)
    } else {
        None
    };
    let setup = builder || preview || !preset.as_ref().is_some_and(|p| p.setup_completed);
    if setup && !preview && !builder && !args.iter().any(|a| a == "--no-uac") {
        platform::authorize_setup()?;
    }
    let engine = engine::Engine::new(&data, preset.map(|p| p.settings))?;
    let icon = image::load_from_memory(include_bytes!("../assets/workflow.png"))?.into_rgba8();
    let (w, h) = icon.dimensions();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!(
                "WorkFlow Media{} · {}",
                if builder { " · Setup Builder" } else { "" },
                updater::VERSION
            ))
            .with_inner_size([1200., 680.])
            .with_min_inner_size([1024., 620.])
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width: w,
                height: h,
            }),
        renderer: eframe::Renderer::Glow,
        vsync: true,
        centered: true,
        ..Default::default()
    };
    let background = args.iter().any(|a| a == "--background");
    eframe::run_native(
        "WorkFlow Media",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::App::new(
                cc,
                engine,
                preset_path,
                data,
                setup,
                preview,
                background,
                initial_signal,
                builder,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    drop(lock);
    Ok(())
}
fn media_command(args: &[String]) -> Result<()> {
    let arg = |key: &str| {
        args.windows(2)
            .find(|w| w[0] == key)
            .map(|w| w[1].clone())
            .with_context(|| format!("Missing {key}"))
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let settings = model::Settings {
        cookies_file: arg("--cookies").unwrap_or_default(),
        cookies_browser: arg("--cookies-from-browser")
            .ok()
            .map(|value| model::Browser::parse(&value))
            .transpose()?,
        ..Default::default()
    };
    if let Ok(url) = arg("--inspect-url") {
        let report = media::inspect(&url, &settings, &cancel)?;
        model::save_json(Path::new(&arg("--output")?), &report)?;
        return Ok(());
    }
    if let Ok(input) = arg("--convert-media") {
        let output = PathBuf::from(arg("--output")?);
        let mut inputs = vec![PathBuf::from(input)];
        if let Ok(audio) = arg("--audio") {
            inputs.push(audio.into());
        }
        return media::convert(&inputs, &output, &arg("--format")?, &cancel);
    }
    let url = arg("--download-url")?;
    let directory = arg("--directory")?;
    let job = model::Job {
        url,
        format: arg("--format").unwrap_or("MP4".into()),
        quality: arg("--quality")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        directory,
        ..Default::default()
    };
    let path = media::download(&job, &settings, &cancel, |_| {})?;
    model::save_json(
        Path::new(&arg("--output")?),
        &serde_json::json!({"file":path,"bytes":fs::metadata(&path)?.len()}),
    )?;
    Ok(())
}
fn read_preset(path: &Path, existing: &model::Settings) -> Result<model::Preset> {
    let read = |p: &Path| -> Result<model::Preset> {
        let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(p)?)?;
        // Old presets do not contain every setting. Preserve the imported queue's fields.
        let mut merged = serde_json::to_value(existing)?;
        if let (Some(base), Some(saved)) = (merged.as_object_mut(), raw.as_object()) {
            for (key, value) in saved {
                base.insert(key.clone(), value.clone());
            }
        }
        if let (Some(object), Some(base)) = (raw.as_object_mut(), merged.as_object()) {
            for (key, value) in base {
                object.entry(key.clone()).or_insert(value.clone());
            }
        }
        let value: model::Preset = serde_json::from_value(raw)?;
        if value.version != 1 {
            bail!("Unsupported preset version / Nepodporovaná verze předvolby");
        }
        Ok(value)
    };
    read(path).or_else(|_|read(&PathBuf::from(format!("{}.bak",path.display())))).context("Cannot read preset; original file preserved / Předvolbu nelze načíst; původní soubor zachován")
}
fn sign_release(args: &[String]) -> Result<()> {
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::{Digest, Sha256};
    let arg = |key: &str| {
        args.windows(2)
            .find(|w| w[0] == key)
            .map(|w| w[1].clone())
            .with_context(|| format!("Missing {key}"))
    };
    let raw = fs::read(arg("--key")?)?;
    let key = SigningKey::from_bytes(
        &raw.try_into()
            .map_err(|_| anyhow::anyhow!("Signing key must be exactly 32 bytes"))?,
    );
    if let Ok(output) = arg("--public-key") {
        fs::write(output, hex::encode(key.verifying_key().to_bytes()))?;
        return Ok(());
    }
    let version = arg("--release-version")?;
    semver::Version::parse(&version)?;
    let file = PathBuf::from(arg("--file")?);
    let name = file
        .file_name()
        .context("Release asset name")?
        .to_string_lossy();
    let mut m = updater::Manifest {
        version: version.clone(),
        target: updater::target().into(),
        url: format!(
            "https://github.com/{}/releases/download/v{version}/{name}",
            updater::REPOSITORY
        ),
        sha256: hex::encode(Sha256::digest(fs::read(&file)?)),
        signature: String::new(),
    };
    m.signature = hex::encode(key.sign(&m.signed_bytes()).to_bytes());
    model::save_json(Path::new(&arg("--output")?), &m)?;
    Ok(())
}
