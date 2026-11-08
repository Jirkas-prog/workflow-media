//! Small, native Rust web installer. No media tools or application UI are linked in.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)] // Shared installation/model modules also serve the full application.
#[path = "../installer.rs"]
mod installer;
#[path = "../model.rs"]
mod model;
#[path = "../platform.rs"]
mod platform;
#[path = "../style.rs"]
mod style;
#[path = "../updater.rs"]
mod updater;
use anyhow::{Context, Result, bail};

fn main() {
    if let Err(error) = run() {
        let text = format!("{error:#}");
        eprintln!("{text}");
        #[cfg(windows)]
        if !std::env::args().any(|a| a == "--test-install-to") {
            installer::compact::message(&text, true);
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--authorize-setup") {
        return Ok(());
    }
    if args.iter().any(|a| a == "--version") {
        println!("WorkFlow Media Setup {} · Rust", updater::VERSION);
        return Ok(());
    }
    let (_, package) = installer::read_package(&std::env::current_exe()?)?
        .context("Missing installer settings / Chybí nastavení instalátoru")?;
    let installer::Package::Installer(profile) = package else {
        bail!("Invalid installer package / Neplatný instalační balíček");
    };
    if let Some(pair) = args.windows(2).find(|p| p[0] == "--test-install-to") {
        println!(
            "{}",
            installer::test_install(&profile, std::path::Path::new(&pair[1]))?.display()
        );
        return Ok(());
    }
    #[cfg(windows)]
    return installer::compact::run(profile);
    #[cfg(not(windows))]
    bail!("This installer targets Windows x64")
}
