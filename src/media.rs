//! yt-dlp performs extraction, transfer, resume and post-processing; Rust owns the queue.
use crate::{
    model::{self, Job, Settings},
    platform, process, tools,
};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub enum Update {
    Title(String),
    Progress {
        done: u64,
        total: u64,
        speed: f64,
    },
    Processing,
    CookieProfile {
        index: usize,
        total: usize,
        label: String,
    },
}
fn command(t: &tools::Tools, s: &Settings) -> Result<Command> {
    let mut c = platform::command(&t.downloader);
    c.args([
        "--ignore-config",
        "--no-playlist",
        "--no-colors",
        "--encoding",
        "utf-8",
        "--socket-timeout",
        "20",
        "--retries",
        "3",
        "--fragment-retries",
        "3",
        "--no-cache-dir",
        "--no-js-runtimes",
    ]);
    c.arg("--js-runtimes")
        .arg(format!("deno:{}", t.deno.display()));
    c.arg("--ffmpeg-location").arg(&t.ffmpeg);
    if let Some(browser) = s.cookies_browser {
        c.arg("--cookies-from-browser").arg(browser.argument());
    } else if !s.cookies_file.is_empty() {
        c.arg("--cookies").arg(&s.cookies_file);
    }
    Ok(c)
}
fn selector(format: &str, quality: i32) -> Result<String> {
    if !matches!(format, "MP4" | "MKV" | "MP3") || !model::QUALITIES.contains(&quality) {
        bail!("Invalid format or quality / Neplatný formát nebo kvalita");
    }
    if format == "MP3" {
        return Ok("ba/b".into());
    }
    let cap = if quality > 0 {
        format!("[height<=?{quality}]")
    } else {
        String::new()
    };
    Ok(format!("bv*{cap}+ba/b{cap}"))
}
pub fn inspect(url: &str, settings: &Settings, cancel: &AtomicBool) -> Result<Value> {
    crate::cookies::with_auth(
        settings,
        cancel,
        |_, _, _| {},
        |s, _| inspect_one(url, s, cancel),
    )
}
fn inspect_one(url: &str, settings: &Settings, cancel: &AtomicBool) -> Result<Value> {
    model::parse_url(url)?;
    let t = tools::get()?;
    let mut c = command(&t, settings)?;
    c.args(["--dump-single-json", "--skip-download", "--", url]);
    let mut report = None;
    process::run(&mut c, cancel, Some(Duration::from_secs(150)), |line| {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            let formats: Vec<_> = v["formats"].as_array().into_iter().flatten().map(|f| serde_json::json!({"container":f["ext"], "height":f["height"], "audio":f["acodec"].as_str().is_some_and(|s|s!="none"), "bitrate":f["tbr"]})).collect();
            // Signed media URLs and authentication headers never enter the public report.
            report = Some(serde_json::json!({"title":v["title"], "formats":formats}));
        }
    })?;
    report.context("No media metadata / Chybí údaje o videu")
}
pub fn download(
    job: &Job,
    settings: &Settings,
    cancel: &AtomicBool,
    progress: impl Fn(Update) + Sync,
) -> Result<PathBuf> {
    model::parse_url(&job.url)?;
    crate::cookies::with_auth(
        settings,
        cancel,
        |index, total, label| {
            progress(Update::CookieProfile {
                index,
                total,
                label: label.into(),
            })
        },
        |settings, identity| download_one(job, settings, identity, cancel, &progress),
    )
}
fn download_one(
    job: &Job,
    settings: &Settings,
    identity: &str,
    cancel: &AtomicBool,
    progress: &(impl Fn(Update) + Sync),
) -> Result<PathBuf> {
    let format = selector(&job.format, job.quality)?;
    if !Path::new(&job.directory).is_absolute() {
        bail!("Choose an absolute download folder / Vyberte úplnou cestu ke složce stahování");
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("@cancelled");
    }
    let t = tools::get()?;
    let dir = Path::new(&job.directory);
    let base = dir
        .join(".workflow-partials")
        .join(model::safe_name(&job.id));
    let partial = if identity == "default" {
        base
    } else {
        base.join(identity)
    };
    fs::create_dir_all(&partial)?;
    let mut s = settings.clone();
    // yt-dlp may update a Netscape file. Keep the user's original export unchanged.
    let cookie_copy = if s.cookies_browser.is_none() && !s.cookies_file.is_empty() {
        let file = tempfile::NamedTempFile::new()?;
        fs::copy(&s.cookies_file, file.path())?;
        s.cookies_file = file.path().to_string_lossy().into_owned();
        Some(file)
    } else {
        None
    };
    let mut c = command(&t, &s)?;
    c.args([
        "--no-simulate",
        "--no-overwrites",
        "--continue",
        "--no-mtime",
        "--no-keep-video",
        "--newline",
        "--progress",
        "--progress-delta",
        "0.25",
        "--no-write-info-json",
        "--no-write-thumbnail",
        "--no-write-subs",
        "--windows-filenames",
    ]);
    c.args(["--match-filter", "!is_live", "-f", &format]);
    c.arg("--paths").arg(&partial).args(["-o", "media.%(ext)s"]);
    if job.format == "MP3" {
        c.args(["-x", "--audio-format", "mp3", "--audio-quality", "0"]);
    } else {
        let ext = job.format.to_ascii_lowercase();
        c.args(["--merge-output-format", &ext, "--remux-video", &ext]);
    }
    c.args(["--print", "before_dl:WFM_TITLE:%(title)j", "--print", "post_process:WFM_PROCESS", "--print", "after_move:WFM_FILE:%(filepath)j", "--progress-template", "download:WFM_PROGRESS:%(progress.downloaded_bytes)j\t%(progress.total_bytes)j\t%(progress.total_bytes_estimate)j\t%(progress.speed)j"]);
    c.arg("--").arg(&job.url);
    let mut title = job.title.clone();
    let mut output = None;
    process::run(&mut c, cancel, None, |line| {
        if let Some(value) = line
            .strip_prefix("WFM_TITLE:")
            .and_then(|s| serde_json::from_str::<String>(s).ok())
        {
            title = value.clone();
            progress(Update::Title(value));
        } else if line == "WFM_PROCESS" {
            progress(Update::Processing);
        } else if let Some(path) = line
            .strip_prefix("WFM_FILE:")
            .and_then(|s| serde_json::from_str::<String>(s).ok())
        {
            output = Some(PathBuf::from(path));
        } else if let Some(p) = parse_progress(line) {
            progress(p);
        }
    })?;
    drop(cookie_copy);
    if cancel.load(Ordering::Relaxed) {
        bail!("@cancelled");
    }
    let output = output.context("No output file; the link may be a live stream / Chybí výsledný soubor; odkaz může být živý přenos")?;
    let output = output.canonicalize()?;
    let expected = partial.canonicalize()?;
    if output.parent() != Some(expected.as_path())
        || !output.is_file()
        || output.extension().and_then(|s| s.to_str()) != Some(&job.format.to_ascii_lowercase())
    {
        bail!("Invalid output path / Neplatná cesta výsledného souboru");
    }
    progress(Update::Processing);
    let target = publish(
        &output,
        dir,
        &title,
        &job.format.to_ascii_lowercase(),
        cancel,
    )?;
    // Only our own, canonical per-job directory is removed after a completed transfer.
    let _ = fs::remove_dir_all(&expected);
    Ok(target)
}
fn parse_progress(line: &str) -> Option<Update> {
    let values: Vec<_> = line.strip_prefix("WFM_PROGRESS:")?.split('\t').collect();
    if values.len() != 4 {
        return None;
    }
    let number = |s: &str| {
        s.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.)
            .unwrap_or(0.)
    };
    let total = number(values[1]).max(number(values[2]));
    Some(Update::Progress {
        done: number(values[0]) as u64,
        total: total as u64,
        speed: number(values[3]),
    })
}
fn publish(
    source: &Path,
    dir: &Path,
    title: &str,
    extension: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    // Write a temporary file on the same volume, then publish without replacing another download.
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    let mut input = fs::File::open(source)?;
    let mut buf = [0; 131072];
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("@cancelled");
        }
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        temp.write_all(&buf[..n])?;
    }
    temp.as_file().sync_all()?;
    let name = model::safe_name(title);
    for n in 0..10000 {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let target = dir.join(format!("{name}{suffix}.{extension}"));
        match temp.persist_noclobber(&target) {
            Ok(_) => return Ok(target),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temp = e.file;
            }
            Err(e) => return Err(e.error.into()),
        }
    }
    bail!("Too many files with the same title / Příliš mnoho souborů se stejným názvem")
}
pub fn convert(inputs: &[PathBuf], output: &Path, format: &str, cancel: &AtomicBool) -> Result<()> {
    selector(format, 0)?;
    if output.exists() {
        bail!("Output already exists / Výsledný soubor již existuje");
    }
    let t = tools::get()?;
    let mut c = platform::command(t.ffmpeg);
    c.args(["-hide_banner", "-nostdin", "-n"]);
    for input in inputs {
        c.arg("-i").arg(input);
    }
    if format == "MP3" {
        c.args(["-vn", "-c:a", "libmp3lame", "-q:a", "0"]);
    } else {
        if inputs.len() > 1 {
            c.args(["-map", "0:v:0", "-map", "1:a:0"]);
        }
        c.args(["-c", "copy"]);
    }
    c.arg(output);
    let result = process::run(&mut c, cancel, None, |_| {});
    if result.is_err() {
        let _ = fs::remove_file(output);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Browser;
    #[test]
    #[ignore = "requires media tools and WFM_TEST_MEDIA pointing to a local MP4 fixture"]
    fn real_downloader_resumes_http_range_after_cancellation() {
        use sha2::{Digest, Sha256};
        use std::{
            io::{BufRead, BufReader},
            net::TcpListener,
            sync::{Arc, Mutex},
        };
        let fixture =
            fs::read(std::env::var_os("WFM_TEST_MEDIA").expect("WFM_TEST_MEDIA")).unwrap();
        assert!(fixture.len() > 512 * 1024);
        let data = Arc::new(fixture);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let data = data.clone();
            let ranges = ranges.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut workers = Vec::new();
                while !stop.load(Ordering::Relaxed) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(10)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(10)))
                        .unwrap();
                    let data = data.clone();
                    let ranges = ranges.clone();
                    workers.push(std::thread::spawn(move || {
                        let mut reader = BufReader::new(stream.try_clone().unwrap());
                        let mut first = String::new(); if reader.read_line(&mut first).is_err() { return; }
                        let mut start = None;
                        loop {
                            let mut line = String::new(); if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" { break; }
                            if let Some(range) = line.to_ascii_lowercase().strip_prefix("range: bytes=") { start = range.split('-').next().and_then(|s|s.parse::<usize>().ok()); }
                        }
                        let offset = start.unwrap_or(0).min(data.len());
                        ranges.lock().unwrap().push(offset);
                        let headers = if start.is_some() { format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\n", offset, data.len()-1, data.len()) } else { "HTTP/1.1 200 OK\r\n".into() };
                        if write!(stream, "{headers}Content-Type: video/mp4\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n", data.len()-offset).is_err() { return; }
                        if first.starts_with("HEAD ") { return; }
                        for chunk in data[offset..].chunks(16384) {
                            if stream.write_all(chunk).is_err() { break; }
                            std::thread::sleep(Duration::from_millis(8));
                        }
                    }));
                }
                for worker in workers {
                    let _ = worker.join();
                }
            })
        };
        let root = tempfile::tempdir().unwrap();
        let job = Job {
            url: format!("http://{address}/fixture.mp4"),
            directory: root.path().display().to_string(),
            ..Job::default()
        };
        let cancel = AtomicBool::new(false);
        let first = download(&job, &Settings::default(), &cancel, |p| {
            if matches!(p, Update::Progress {done, ..} if done > 131072) {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        let second = if cancel.load(Ordering::Relaxed) {
            cancel.store(false, Ordering::Relaxed);
            download(&job, &Settings::default(), &cancel, |_| {})
        } else {
            Err(anyhow::anyhow!("No progress event received"))
        };
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        assert_eq!(first.unwrap_err().to_string(), "@cancelled");
        let output = second.unwrap();
        assert_eq!(
            Sha256::digest(fs::read(output).unwrap()),
            Sha256::digest(data.as_slice())
        );
        assert!(
            ranges.lock().unwrap().iter().any(|offset| *offset > 0),
            "Resume must request a nonzero HTTP range"
        );
    }
    #[test]
    fn formats_and_authentication_are_explicit_arguments() {
        assert_eq!(
            selector("MP4", 1080).unwrap(),
            "bv*[height<=?1080]+ba/b[height<=?1080]"
        );
        assert_eq!(selector("MKV", 0).unwrap(), "bv*+ba/b");
        assert_eq!(selector("MP3", 0).unwrap(), "ba/b");
        assert!(selector("--exec", 0).is_err());
        let t = tools::Tools {
            downloader: "yt-dlp".into(),
            ffmpeg: "ffmpeg".into(),
            deno: "deno".into(),
        };
        let mut s = Settings::default();
        s.cookies_file = "export with spaces.txt".into();
        let args: Vec<_> = command(&t, &s)
            .unwrap()
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.windows(2)
                .any(|w| w == ["--cookies", "export with spaces.txt"])
        );
        for browser in Browser::ALL {
            s.cookies_browser = Some(browser);
            let args: Vec<_> = command(&t, &s)
                .unwrap()
                .get_args()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            assert!(
                args.windows(2)
                    .any(|w| w == ["--cookies-from-browser", browser.argument()])
            );
            assert!(!args.iter().any(|s| s == "--cookies"));
        }
    }
    #[test]
    fn structured_progress_handles_unknown_size() {
        match parse_progress("WFM_PROGRESS:123\tnull\t456\t789.5").unwrap() {
            Update::Progress { done, total, speed } => {
                assert_eq!((done, total), (123, 456));
                assert_eq!(speed, 789.5);
            }
            _ => panic!(),
        }
        assert!(parse_progress("ordinary output").is_none());
    }
    #[test]
    fn completed_files_never_replace_existing_names_and_cancel_is_clean() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("input");
        fs::write(&source, b"new").unwrap();
        let old = root.path().join("Title.mp4");
        fs::write(&old, b"old").unwrap();
        let cancel = AtomicBool::new(false);
        let target = publish(&source, root.path(), "Title", "mp4", &cancel).unwrap();
        assert_eq!(fs::read(old).unwrap(), b"old");
        assert_eq!(fs::read(target).unwrap(), b"new");
        cancel.store(true, Ordering::Relaxed);
        assert!(publish(&source, root.path(), "Cancelled", "mp4", &cancel).is_err());
        assert!(!root.path().join("Cancelled.mp4").exists());
    }
}
