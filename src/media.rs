//! Native extraction, HTTP transfer and media processing. No external executables.
use crate::model::{self, Job, Settings};
use anyhow::{Context, Result, bail};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use ytdown::{Format, FormatKind, MediaInfo};

#[derive(Clone, Debug)]
pub struct Media {
    pub title: String,
    pub id: String,
    pub formats: Vec<Format>,
}
pub enum Update {
    Title(String),
    Progress { done: u64, total: u64, speed: f64 },
    Processing,
}
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}
fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(7))
        .build()?)
}
pub fn resolve(url: &str, cookies: &str, cancel: &AtomicBool) -> Result<Media> {
    let rt = runtime()?;
    rt.block_on(async {
        tokio::select! {r=resolve_async(url,cookies)=>r,_=cancelled(cancel)=>bail!("@cancelled")}
    })
}
async fn cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
async fn resolve_async(raw: &str, cookies: &str) -> Result<Media> {
    let url = model::parse_url(raw)?;
    let source = model::source(&url);
    let client = client()?;
    if matches!(source, "YouTube" | "Instagram" | "TikTok")
        || url.host_str().is_some_and(|h| {
            h == "x.com"
                || h.ends_with(".twitter.com")
                || h == "twitter.com"
                || h == "reddit.com"
                || h.ends_with(".reddit.com")
        })
    {
        let mut builder = ytdown::Ytdown::builder().client(client.clone());
        if !cookies.is_empty() {
            builder = builder.cookies(ytdown::cookies::CookieJar::parse_netscape(
                &fs::read_to_string(cookies)?,
            )?);
        }
        let yt = builder.build()?;
        let result = tokio::time::timeout(Duration::from_secs(90), yt.resolve(raw))
            .await
            .context("Video lookup timed out / Vyhledání videa vypršelo")??;
        let video = match result {
            MediaInfo::Single(v) => v,
            MediaInfo::Collection(mut c) => {
                let entry = c
                    .entries
                    .next()
                    .await
                    .context("Empty collection / Prázdný seznam")??;
                match yt.resolve(&entry.url).await? {
                    MediaInfo::Single(v) => v,
                    _ => bail!(
                        "Use a link to an individual video / Použijte odkaz na konkrétní video"
                    ),
                }
            }
        };
        if video.is_live {
            bail!(
                "Live streams are not supported in this version / Živé přenosy tato verze nepodporuje"
            );
        }
        return Ok(Media {
            title: video.title,
            id: video.id,
            formats: video.formats,
        });
    }
    let mut request = client.get(url.clone());
    if !cookies.is_empty() {
        let jar = ytdown::cookies::CookieJar::parse_netscape(&fs::read_to_string(cookies)?)?;
        if let Some(header) = jar.header_for(
            url.host_str().unwrap_or(""),
            url.scheme() == "https",
            model::now().timestamp().max(0) as u64,
        ) {
            request = request.header(reqwest::header::COOKIE, header);
        }
    }
    let response = request.send().await?.error_for_status()?;
    let content = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let final_url = response.url().clone();
    if content.starts_with("video/")
        || content.starts_with("audio/")
        || ["mp4", "webm", "mkv", "m4a", "mp3", "wav", "flac"]
            .iter()
            .any(|ext| {
                url.path()
                    .to_ascii_lowercase()
                    .ends_with(&format!(".{ext}"))
            })
    {
        let mut f = Format::default();
        f.url = final_url.to_string();
        f.mime_type = Some(content.clone());
        f.filesize = response.content_length();
        f.container = Some(if url.path().ends_with(".webm") {
            ytdown::Container::WebM
        } else {
            ytdown::Container::Mp4
        });
        let audio = content.starts_with("audio/")
            || ["m4a", "mp3", "wav", "flac"]
                .iter()
                .any(|x| url.path().ends_with(&format!(".{x}")));
        f.audio = Some(Default::default());
        if !audio {
            f.video = Some(Default::default());
        }
        let name = final_url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("video")
            .to_owned();
        return Ok(Media {
            title: name,
            id: uuid::Uuid::new_v4().simple().to_string()[..10].into(),
            formats: vec![f],
        });
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        if body.len() + bytes.len() > 12_000_000 {
            bail!("Page is too large / Stránka je příliš velká");
        }
        body.extend_from_slice(&bytes);
    }
    let html = String::from_utf8_lossy(&body);
    let mut media = extract_page(&html, &final_url)?;
    if source == "Vimeo" {
        let id = regex::Regex::new(r"/(\d+)")?
            .captures(final_url.path())
            .map(|c| c[1].to_owned());
        if let Some(id) = id {
            let config = client
                .get(format!("https://player.vimeo.com/video/{id}/config"))
                .header("Referer", final_url.as_str())
                .send()
                .await?
                .error_for_status()?
                .json::<serde_json::Value>()
                .await?;
            if let Some(title) = config["video"]["title"].as_str() {
                media.title = title.into();
            }
            if let Some(rows) = config["request"]["files"]["progressive"].as_array() {
                for row in rows {
                    if let Some(url) = row["url"].as_str() {
                        let mut f = progressive(url, row["height"].as_u64().unwrap_or(0) as u32);
                        f.http_headers
                            .push(("Referer".into(), final_url.to_string()));
                        media.formats.push(f);
                    }
                }
            }
        }
    }
    if media.formats.is_empty() {
        bail!(
            "No accessible video found. The service may require cookies or may have changed its page. / Nebylo nalezeno dostupné video. Služba může vyžadovat cookies nebo změnila stránku."
        );
    }
    Ok(media)
}
fn progressive(url: &str, height: u32) -> Format {
    let mut f = Format::default();
    f.url = url.into();
    f.container = Some(ytdown::Container::Mp4);
    f.video = Some(Default::default());
    f.video.as_mut().unwrap().height = (height > 0).then_some(height);
    f.audio = Some(Default::default());
    f
}
fn json_value(raw: &str) -> String {
    serde_json::from_str::<String>(&format!("\"{raw}\""))
        .unwrap_or_else(|_| html_escape::decode_html_entities(raw).into_owned())
}
pub fn extract_page(html: &str, base: &url::Url) -> Result<Media> {
    let mut formats = Vec::new();
    let mut title = String::from("Video");
    if let Some(c) = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>")?.captures(html) {
        title = html_escape::decode_html_entities(&c[1]).trim().into();
    }
    // HD is a preference label, not a guaranteed resolution. Preserve source priority on ties.
    for c in regex::Regex::new(r#"(?s)"(?:browser_native_hd_url|playable_url_quality_hd|hd_src)"\s*:\s*"((?:\\.|[^"\\])*)""#)?.captures_iter(html){formats.push(progressive(&json_value(&c[1]),0));}
    for c in regex::Regex::new(r#"(?s)"(?:browser_native_sd_url|playable_url|sd_src|video_url)"\s*:\s*"((?:\\.|[^"\\])*)""#)?.captures_iter(html){formats.push(progressive(&json_value(&c[1]),0));}
    for c in regex::Regex::new(
        r#"(?s)"(?:dash_manifest|dash_manifest_xml_string)"\s*:\s*"((?:\\.|[^"\\])*)""#,
    )?
    .captures_iter(html)
    {
        formats.extend(dash_formats(&json_value(&c[1]), base)?);
    }
    for tag in regex::Regex::new(r"(?is)<(?:meta|source|video)\b[^>]*>")?.find_iter(html) {
        let tag = tag.as_str();
        let property = attribute(tag, "property");
        if property == "og:title" {
            title = attribute(tag, "content");
        }
        let raw = if property.starts_with("og:video")
            && !property.ends_with(":type")
            && !property.ends_with(":width")
            && !property.ends_with(":height")
        {
            attribute(tag, "content")
        } else if tag.to_lowercase().starts_with("<source")
            || tag.to_lowercase().starts_with("<video")
        {
            attribute(tag, "src")
        } else {
            String::new()
        };
        if !raw.is_empty() {
            if let Ok(url) = base.join(&raw) {
                formats.push(progressive(url.as_str(), 0));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    formats.retain(|f| model::parse_url(&f.url).is_ok() && seen.insert(f.url.clone()));
    let id = regex::Regex::new(r"\d{5,}")?
        .find(base.path())
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| {
            use sha2::Digest;
            hex::encode(sha2::Sha256::digest(base.as_str()))[..10].into()
        });
    Ok(Media { title, id, formats })
}
fn attribute(tag: &str, name: &str) -> String {
    regex::Regex::new(&format!(
        r#"(?is)\b{}\s*=\s*["']([^"']*)["']"#,
        regex::escape(name)
    ))
    .unwrap()
    .captures(tag)
    .map(|c| html_escape::decode_html_entities(&c[1]).into_owned())
    .unwrap_or_default()
}
fn dash_formats(xml: &str, base: &url::Url) -> Result<Vec<Format>> {
    let xml = html_escape::decode_html_entities(xml);
    let doc = roxmltree::Document::parse(&xml)?;
    if doc
        .descendants()
        .any(|n| n.has_tag_name("ContentProtection"))
    {
        bail!("Encrypted media is not supported / Šifrovaná média nejsou podporována");
    }
    let mut formats = vec![];
    for node in doc
        .descendants()
        .filter(|n| n.has_tag_name("Representation"))
    {
        let Some(raw) = node
            .children()
            .find(|n| n.has_tag_name("BaseURL"))
            .and_then(|n| n.text())
        else {
            continue;
        };
        let parent = node.parent().unwrap_or(node);
        let attr = |key| {
            node.attribute(key)
                .or_else(|| parent.attribute(key))
                .unwrap_or("")
        };
        let mime = attr("mimeType");
        let mut f = Format::default();
        f.url = base.join(raw)?.to_string();
        f.bitrate = attr("bandwidth").parse().ok();
        f.mime_type = Some(mime.into());
        f.container = Some(ytdown::Container::Mp4);
        if mime.starts_with("video") {
            let mut v = ytdown::VideoStream::default();
            v.height = attr("height").parse().ok();
            v.width = attr("width").parse().ok();
            v.codec = attr("codecs").into();
            f.video = Some(v);
        } else if mime.starts_with("audio") {
            let mut a = ytdown::AudioStream::default();
            a.codec = attr("codecs").into();
            a.bitrate = f.bitrate;
            f.audio = Some(a);
        }
        formats.push(f);
    }
    Ok(formats)
}
pub fn select(media: &Media, format: &str, quality: i32) -> Result<Vec<Format>> {
    let height = |f: &Format| f.video.as_ref().and_then(|v| v.height).unwrap_or(0);
    let audio_score = |f: &Format| {
        f.audio
            .as_ref()
            .and_then(|a| a.bitrate)
            .or(f.bitrate)
            .unwrap_or(0)
    };
    if format == "MP3" {
        let best = media
            .formats
            .iter()
            .filter(|f| {
                f.audio.is_some()
                    && f.audio
                        .as_ref()
                        .is_some_and(|a| !a.codec.to_lowercase().contains("opus"))
            })
            .max_by_key(|f| (f.video.is_none(), audio_score(f)))
            .context("No decodable audio stream / Chybí podporovaný zvukový proud")?;
        return Ok(vec![best.clone()]);
    }
    let mut videos: Vec<_> = media
        .formats
        .iter()
        .filter(|f| {
            f.video.is_some() && (quality == 0 || height(f) == 0 || height(f) <= quality as u32)
        })
        .collect();
    if format == "MP4"
        && videos
            .iter()
            .any(|f| matches!(f.container, Some(ytdown::Container::Mp4)))
    {
        videos.retain(|f| matches!(f.container, Some(ytdown::Container::Mp4)));
    }
    let video = videos
        .into_iter()
        .enumerate()
        .max_by_key(|(index, f)| (height(f), f.bitrate.unwrap_or(0), std::cmp::Reverse(*index)))
        .map(|(_, f)| f)
        .context("No video at the requested quality / Video v požadované kvalitě není dostupné")?;
    if video.kind() == FormatKind::Progressive {
        return Ok(vec![video.clone()]);
    }
    let audio = media
        .formats
        .iter()
        .filter(|f| f.audio.is_some() && f.video.is_none())
        .max_by_key(|f| {
            (
                matches!(
                    f.container,
                    Some(ytdown::Container::Mp4 | ytdown::Container::M4a)
                ),
                audio_score(f),
            )
        })
        .context("No matching audio stream / Chybí odpovídající zvukový proud")?;
    Ok(vec![video.clone(), audio.clone()])
}
#[derive(Serialize, Deserialize)]
struct Resume {
    url_key: String,
    etag: String,
    total: u64,
}
pub fn download(
    job: &Job,
    settings: &Settings,
    cancel: &AtomicBool,
    progress: impl Fn(Update) + Sync,
) -> Result<PathBuf> {
    let media = resolve(&job.url, &settings.cookies_file, cancel)?;
    if cancel.load(Ordering::Relaxed) {
        bail!("@cancelled");
    }
    progress(Update::Title(media.title.clone()));
    let selected = select(&media, &job.format, job.quality)?;
    let dir = Path::new(&job.directory);
    fs::create_dir_all(dir)?;
    let partial = dir
        .join(".workflow-partials")
        .join(model::safe_name(&job.id));
    fs::create_dir_all(&partial)?;
    let runtime = runtime()?;
    let mut files = vec![];
    let mut prior = 0;
    let total = selected
        .iter()
        .map(|f| f.filesize.unwrap_or(0))
        .sum::<u64>();
    for (i, f) in selected.iter().enumerate() {
        let path = partial.join(format!("stream-{i}.media"));
        runtime.block_on(async {tokio::select! {
            result=async {
                let mut attempt=0;
                loop {
                    match download_stream(f,&path,prior,total,cancel,&progress).await {
                        Ok(())=>break Ok(()),
                        Err(e) if attempt<2 && !cancel.load(Ordering::Relaxed) && e.downcast_ref::<reqwest::Error>().is_some_and(|e|e.is_timeout()||e.is_connect()||e.is_body()||e.status().is_some_and(|s|s.is_server_error()||s.as_u16()==429))=>{
                            attempt+=1;tokio::time::sleep(Duration::from_secs(attempt*2)).await;
                        },
                        Err(e)=>break Err(e),
                    }
                }
            }=>result,
            _=cancelled(cancel)=>bail!("@cancelled")
        }})?;
        prior += fs::metadata(&path)?.len();
        files.push(path);
    }
    drop(runtime);
    if cancel.load(Ordering::Relaxed) {
        bail!("@cancelled");
    }
    progress(Update::Processing);
    let output = partial.join(format!("result.{}", job.format.to_ascii_lowercase()));
    if job.format == "MP3" {
        crate::mux::mp3(&files[0], &output, cancel)?;
    } else {
        crate::mux::merge(&files, &output, &job.format, cancel)?;
    }
    let name = format!(
        "{} [{}] {}{}.{}",
        model::safe_name(&media.title),
        model::safe_name(&media.id),
        job.format,
        if job.quality == 0 {
            "-best".into()
        } else {
            format!("-{}", job.quality)
        },
        job.format.to_ascii_lowercase()
    );
    let mut target = dir.join(name);
    if target.exists() {
        target = dir.join(format!(
            "{} [{}].{}",
            model::safe_name(&media.title),
            job.id,
            job.format.to_ascii_lowercase()
        ));
    }
    let mut dst = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)?;
    let copy = (|| {
        let mut src = fs::File::open(&output)?;
        let mut buf = [0u8; 131072];
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("@cancelled");
            }
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])?;
        }
        dst.sync_all()?;
        Ok(())
    })();
    drop(dst);
    if let Err(e) = copy {
        let _ = fs::remove_file(&target);
        return Err(e);
    }
    for path in files {
        let _ = fs::remove_file(path);
    }
    let _ = fs::remove_file(output);
    Ok(target)
}
async fn download_stream(
    f: &Format,
    path: &Path,
    prior: u64,
    total_hint: u64,
    cancel: &AtomicBool,
    progress: &(impl Fn(Update) + Sync),
) -> Result<()> {
    use sha2::Digest;
    use tokio::io::AsyncWriteExt;
    let client = client()?;
    let url = model::parse_url(&f.url)?;
    let key = hex::encode(sha2::Sha256::digest(format!(
        "{}:{}:{:?}:{:?}",
        url.host_str().unwrap_or(""),
        url.path(),
        f.itag,
        f.filesize
    )));
    let meta = path.with_extension("resume.json");
    let old = fs::read(&meta)
        .ok()
        .and_then(|s| serde_json::from_slice::<Resume>(&s).ok());
    let mut offset = if old
        .as_ref()
        .is_some_and(|m| m.url_key == key && !m.etag.is_empty())
    {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    let mut request = client.get(url).header("Accept-Encoding", "identity");
    for (k, v) in &f.http_headers {
        request = request.header(k, v);
    }
    if offset > 0 {
        request = request
            .header("Range", format!("bytes={offset}-"))
            .header("If-Range", &old.as_ref().unwrap().etag);
    }
    let response = request.send().await?;
    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
        && offset > 0
        && old.as_ref().is_some_and(|m| m.total == offset)
        && response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            == Some(format!("bytes */{offset}").as_str())
    {
        let validator = response
            .headers()
            .get("etag")
            .or_else(|| response.headers().get("last-modified"))
            .and_then(|v| v.to_str().ok());
        if validator == old.as_ref().map(|m| m.etag.as_str()) {
            return Ok(());
        }
    }
    let mut response = response.error_for_status()?;
    if response.status().as_u16() == 206 {
        let range = response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if crate::engine::parse_range(range).0 != Some(offset) {
            bail!("Invalid range response / Neplatný rozsah odpovědi");
        }
    } else {
        offset = 0;
    }
    let total = response.content_length().map(|n| n + offset).unwrap_or(0);
    if total > offset
        && fs2::available_space(path.parent().unwrap()).unwrap_or(u64::MAX)
            < total - offset + 32 * 1024 * 1024
    {
        bail!("Not enough disk space / Nedostatek místa na disku");
    }
    let etag = response
        .headers()
        .get("etag")
        .or_else(|| response.headers().get("last-modified"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    model::save_json(
        &meta,
        &Resume {
            url_key: key,
            etag,
            total,
        },
    )?;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(path)
        .await?;
    let (mut done, mut last_bytes, mut last) = (offset, offset, Instant::now());
    while let Some(chunk) = response.chunk().await? {
        if cancel.load(Ordering::Relaxed) {
            bail!("@cancelled");
        }
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if last.elapsed() >= Duration::from_millis(150) {
            let speed = (done - last_bytes) as f64 / last.elapsed().as_secs_f64();
            progress(Update::Progress {
                done: done + prior,
                total: total_hint.max(total + prior),
                speed,
            });
            last = Instant::now();
            last_bytes = done;
        }
    }
    file.sync_all().await?;
    if done == 0 || (total > 0 && done != total) {
        bail!("Incomplete media transfer / Neúplný přenos média");
    }
    progress(Update::Progress {
        done: done + prior,
        total: total_hint.max(total + prior),
        speed: 0.,
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn facebook_public_fields() {
        let h = r#"<title>Example video</title>{"browser_native_hd_url":"https:\/\/cdn.example.org\/video.mp4?a=1\u0026b=2","browser_native_sd_url":"https:\/\/cdn.example.org\/sd.mp4"}"#;
        let m = extract_page(
            h,
            &url::Url::parse("https://facebook.com/watch/123456").unwrap(),
        )
        .unwrap();
        assert_eq!(m.formats.len(), 2);
        assert!(m.formats[0].url.contains("&b=2"));
        assert_eq!(
            select(&m, "MP4", 0).unwrap()[0]
                .video
                .as_ref()
                .unwrap()
                .height,
            None
        );
        assert!(select(&m, "MP4", 0).unwrap()[0].url.contains("/video.mp4"));
    }
    #[test]
    fn encrypted_dash_refused() {
        assert!(
            dash_formats(
                "<MPD><ContentProtection/></MPD>",
                &url::Url::parse("https://example.com").unwrap()
            )
            .is_err()
        );
    }
}
