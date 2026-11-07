use anyhow::{Context, Result, bail};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
pub const REPOSITORY: &str = "Jirkas-prog/workflow-media";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub target: String,
    pub url: String,
    pub sha256: String,
    pub signature: String,
}
impl Manifest {
    pub fn signed_bytes(&self) -> Vec<u8> {
        format!(
            "{}\n{}\n{}\n{}\n",
            self.version, self.target, self.url, self.sha256
        )
        .into_bytes()
    }
}
pub fn target() -> &'static str {
    if cfg!(windows) {
        "windows-x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    }
}
fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(format!("WorkFlow-Media/{VERSION}"))
        .timeout(Duration::from_secs(90))
        .build()?)
}
fn validate(m: &Manifest, key: &[u8; 32]) -> Result<()> {
    let version = semver::Version::parse(&m.version)?;
    let url = url::Url::parse(&m.url)?;
    let expected = format!("/{REPOSITORY}/releases/download/v{}/", version);
    if m.target != target()
        || url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.path().starts_with(&expected)
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!("Invalid update destination / Neplatný zdroj aktualizace");
    }
    if m.sha256.len() != 64 || !m.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("Invalid update checksum");
    }
    let sig = Signature::from_slice(&hex::decode(&m.signature)?)?;
    VerifyingKey::from_bytes(key)?
        .verify(&m.signed_bytes(), &sig)
        .context("Update signature is not valid / Podpis aktualizace není platný")?;
    Ok(())
}
pub fn latest() -> Result<Manifest> {
    let response = client()?
        .get(format!(
            "https://github.com/{REPOSITORY}/releases/latest/download/update-{}.json",
            target()
        ))
        .send()?;
    if response.status().as_u16() == 404 {
        bail!("No update release is available yet / Aktualizační vydání zatím není dostupné");
    }
    let mut bytes = vec![];
    response
        .error_for_status()?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        bail!("Update manifest too large");
    }
    let m: Manifest = serde_json::from_slice(&bytes)?;
    let key: [u8; 32] = hex::decode(include_str!("../resources/update-key.hex").trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid update key"))?;
    validate(&m, &key)?;
    Ok(m)
}
pub fn check() -> Result<Option<Manifest>> {
    let m = latest()?;
    Ok((semver::Version::parse(&m.version)? > semver::Version::parse(VERSION)?).then_some(m))
}
pub fn download(m: &Manifest, cache: &Path) -> Result<PathBuf> {
    let key: [u8; 32] = hex::decode(include_str!("../resources/update-key.hex").trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid update key"))?;
    validate(m, &key)?;
    fs::create_dir_all(cache)?;
    let path = cache.join(if cfg!(windows) {
        "WorkFlow Media.update.exe"
    } else {
        "workflow-media.update"
    });
    let tmp = path.with_extension("downloading");
    let mut response = client()?.get(&m.url).send()?.error_for_status()?;
    let mut file = fs::File::create(&tmp)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 131072];
    let mut total = 0u64;
    use std::io::Write;
    loop {
        let n = response.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > 1_000_000_000 {
            bail!("Update exceeds size limit");
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n])?;
    }
    file.sync_all()?;
    drop(file);
    if hex::encode(hash.finalize()) != m.sha256 {
        let _ = fs::remove_file(tmp);
        bail!("Update checksum mismatch / Nesouhlasí kontrolní součet aktualizace");
    }
    crate::platform::replace_file(&tmp, &path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    crate::model::save_json(&cache.join("verified-update.json"), m)?;
    Ok(path)
}
pub fn launch_install(download: &Path, exe: &Path, data: &Path, preset: &Path) -> Result<()> {
    crate::platform::command(download)
        .args(["--apply-update", "--destination"])
        .arg(exe)
        .arg("--parent")
        .arg(std::process::id().to_string())
        .arg("--data")
        .arg(data)
        .arg("--preset")
        .arg(preset)
        .spawn()?;
    Ok(())
}
pub fn apply(args: &[String]) -> Result<()> {
    let arg = |key: &str| -> Result<PathBuf> {
        args.windows(2)
            .find(|w| w[0] == key)
            .map(|w| PathBuf::from(&w[1]))
            .with_context(|| format!("Missing {key}"))
    };
    let own = std::env::current_exe()?;
    let destination = arg("--destination")?;
    let data = arg("--data")?;
    let preset = arg("--preset")?;
    if !destination.is_absolute() || destination == own || !destination.is_file() {
        bail!("Invalid application destination");
    }
    let m: Manifest = serde_json::from_slice(&fs::read(
        own.parent()
            .context("Update directory")?
            .join("verified-update.json"),
    )?)?;
    let key: [u8; 32] = hex::decode(include_str!("../resources/update-key.hex").trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Key"))?;
    validate(&m, &key)?;
    if hex::encode(Sha256::digest(fs::read(&own)?)) != m.sha256 {
        bail!("Update executable was modified");
    }
    let pid: u32 = arg("--parent")?.to_string_lossy().parse()?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject},
        };
        let handle = unsafe { OpenProcess(0x00100000, 0, pid) };
        if !handle.is_null() {
            let result = unsafe { WaitForSingleObject(handle, 120000) };
            unsafe {
                CloseHandle(handle);
            }
            if result == 258 {
                bail!("Application did not stop");
            }
        }
    }
    #[cfg(unix)]
    {
        let start = std::time::Instant::now();
        while Path::new(&format!("/proc/{pid}")).exists() {
            if start.elapsed() > Duration::from_secs(120) {
                bail!("Application did not stop");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let staged = destination.with_extension("new");
    fs::copy(&own, &staged)?;
    let backup = destination.with_extension("previous");
    fs::copy(&destination, &backup)?;
    let mut result = Err(anyhow::anyhow!("Cannot replace application"));
    for _ in 0..50 {
        result = crate::platform::replace_file(&staged, &destination);
        if result.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    result?;
    if let Err(e) = crate::platform::command(&destination)
        .arg("--data")
        .arg(data)
        .arg("--preset")
        .arg(preset)
        .spawn()
    {
        let _ = crate::platform::replace_file(&backup, &destination);
        return Err(e.into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn signature_checks_all_fields() {
        let key = SigningKey::from_bytes(&[42; 32]);
        let mut m = Manifest {
            version: "9.0.0".into(),
            target: target().into(),
            url: format!(
                "https://github.com/{REPOSITORY}/releases/download/v9.0.0/WorkFlow-Media.exe"
            ),
            sha256: "a".repeat(64),
            signature: String::new(),
        };
        m.signature = hex::encode(key.sign(&m.signed_bytes()).to_bytes());
        assert!(validate(&m, &key.verifying_key().to_bytes()).is_ok());
        m.sha256 = "b".repeat(64);
        assert!(validate(&m, &key.verifying_key().to_bytes()).is_err());
        m.url = "https://evil.example/program.exe".into();
        assert!(validate(&m, &key.verifying_key().to_bytes()).is_err());
    }
}
