//! Windows carries its verified helper bundle; Linux uses explicitly installed tools.
#[cfg(any(not(windows), bundled_tools, test))]
use anyhow::Context;
use anyhow::Result;
#[cfg(any(windows, test))]
use anyhow::bail;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Tools {
    pub downloader: PathBuf,
    pub ffmpeg: PathBuf,
    pub deno: PathBuf,
}
pub fn get() -> Result<Tools> {
    #[cfg(all(windows, bundled_tools))]
    {
        static READY: std::sync::OnceLock<Tools> = std::sync::OnceLock::new();
        static INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        if let Some(t) = READY.get() {
            return Ok(t.clone());
        }
        let _guard = INIT
            .lock()
            .map_err(|_| anyhow::anyhow!("Tool initialization failed"))?;
        if let Some(t) = READY.get() {
            return Ok(t.clone());
        }
        let data = include_bytes!("../resources/tools.zip");
        let folder = unpack(data, &crate::platform::app_data().join("tools"))?;
        let t = Tools {
            downloader: folder.join("yt-dlp.exe"),
            ffmpeg: folder.join("ffmpeg.exe"),
            deno: folder.join("deno.exe"),
        };
        let _ = READY.set(t.clone());
        return Ok(t);
    }
    #[cfg(all(windows, not(bundled_tools)))]
    bail!(
        "Media tools are not bundled. Build with build-windows.ps1 / Nástroje nejsou přibalené. Sestavte aplikaci pomocí build-windows.ps1"
    );
    #[cfg(not(windows))]
    {
        fn find(name: &str) -> Result<PathBuf> {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|p| p.join(name))
                .find(|p| p.is_absolute() && p.is_file())
                .with_context(|| format!("Install {name} and add it to PATH (see BUILD-LINUX.md) / Nainstalujte {name} a přidejte jej do PATH (viz BUILD-LINUX.cs.md)"))
        }
        Ok(Tools {
            downloader: find("yt-dlp")?,
            ffmpeg: find("ffmpeg")?,
            deno: find("deno")?,
        })
    }
}

#[cfg(any(all(windows, bundled_tools), test))]
fn unpack(data: &[u8], root: &std::path::Path) -> Result<PathBuf> {
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs,
        io::{Cursor, Read},
    };
    fs::create_dir_all(root)?;
    let id = hex::encode(Sha256::digest(data));
    let folder = root.join(&id);
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(format!("{id}.lock")))?;
    fs2::FileExt::lock_exclusive(&lock)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(data))?;
    let mut manifest = Vec::new();
    archive
        .by_name("manifest.json")?
        .take(65537)
        .read_to_end(&mut manifest)?;
    if manifest.len() > 65536 {
        bail!("Tool manifest too large");
    }
    let manifest: BTreeMap<String, String> = serde_json::from_slice(&manifest)?;
    for (name, hash) in &manifest {
        if name.is_empty()
            || name.contains(['/', '\\', ':'])
            || name.starts_with('.')
            || hash.len() != 64
            || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            bail!("Invalid tool manifest");
        }
    }
    for name in ["yt-dlp.exe", "deno.exe", "ffmpeg.exe", "ffprobe.exe"] {
        if !manifest.contains_key(name) {
            bail!("Missing bundled tool: {name}");
        }
    }
    fn matches(path: &std::path::Path, hash: &str) -> bool {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return false;
        };
        if !meta.is_file() || meta.file_type().is_symlink() {
            return false;
        }
        let Ok(mut file) = fs::File::open(path) else {
            return false;
        };
        let mut digest = Sha256::new();
        std::io::copy(&mut file, &mut digest).is_ok() && hex::encode(digest.finalize()) == hash
    }
    if folder.is_dir()
        && manifest
            .iter()
            .all(|(name, hash)| matches(&folder.join(name), hash))
    {
        return Ok(folder);
    }
    // Extract into a private sibling and verify everything before publishing the directory.
    let pending = tempfile::Builder::new()
        .prefix("extract-")
        .tempdir_in(root)?;
    for (name, hash) in &manifest {
        let mut entry = archive
            .by_name(name)
            .with_context(|| format!("Missing tool {name}"))?;
        if entry.size() > 600_000_000 {
            bail!("Bundled tool too large");
        }
        let path = pending.path().join(name);
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        std::io::copy(&mut entry, &mut file)?;
        file.sync_all()?;
        drop(file);
        if !matches(&path, hash) {
            bail!("Bundled tool checksum mismatch: {name}");
        }
    }
    if folder.exists() {
        // Never recursively delete a potentially altered cache directory.
        let quarantine = root.join(format!("invalid-{}", uuid::Uuid::new_v4()));
        fs::rename(&folder, quarantine)
            .context("Cannot repair tool cache / Nelze opravit mezipaměť nástrojů")?;
    }
    fs::rename(pending.path(), &folder)?;
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        io::{Cursor, Write},
    };
    fn bundle(bad: bool) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        let mut manifest = BTreeMap::new();
        for name in ["yt-dlp.exe", "deno.exe", "ffmpeg.exe", "ffprobe.exe"] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"verified-tool").unwrap();
            manifest.insert(name, hex::encode(Sha256::digest(b"verified-tool")));
        }
        if bad {
            manifest.insert("../escape.exe", "a".repeat(64));
        }
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn cache_is_verified_repaired_and_path_safe() {
        let root = tempfile::tempdir().unwrap();
        let data = bundle(false);
        let p = unpack(&data, root.path()).unwrap();
        assert_eq!(unpack(&data, root.path()).unwrap(), p);
        std::fs::write(p.join("deno.exe"), b"tampered").unwrap();
        assert_eq!(unpack(&data, root.path()).unwrap(), p);
        assert_eq!(std::fs::read(p.join("deno.exe")).unwrap(), b"verified-tool");
        assert!(unpack(&bundle(true), root.path()).is_err());
        assert!(!root.path().join("escape.exe").exists());
    }
}
