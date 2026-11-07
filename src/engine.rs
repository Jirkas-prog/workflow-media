use crate::{
    fastshare::FastShare,
    model::{self, Job, JobState, Settings, State},
};
use anyhow::{Context, Result, bail};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

struct Inner {
    state: Mutex<State>,
    file: PathBuf,
    stopping: AtomicBool,
    cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    file_operations: Mutex<HashSet<String>>,
    notice: Mutex<String>,
    scheduler: Mutex<Option<thread::JoinHandle<()>>>,
}
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
    pub fast: FastShare,
}
impl Engine {
    pub fn new(data: &Path, settings: Option<Settings>) -> Result<Self> {
        fs::create_dir_all(data)?;
        let file = data.join("queue.json");
        let mut state = model::load_state(&file)?;
        if let Some(mut s) = settings {
            s.queue_paused = state.settings.queue_paused;
            state.settings = s;
        }
        let engine = Self {
            inner: Arc::new(Inner {
                state: Mutex::new(state),
                file,
                stopping: AtomicBool::new(false),
                cancellations: Mutex::new(HashMap::new()),
                file_operations: Mutex::new(HashSet::new()),
                notice: Mutex::new(String::new()),
                scheduler: Mutex::new(None),
            }),
            fast: FastShare::new(data),
        };
        Ok(engine)
    }
    pub fn start(&self) {
        let mut slot = self.inner.scheduler.lock().unwrap();
        if slot.is_none() {
            self.inner.stopping.store(false, Ordering::Relaxed);
            let worker = self.clone();
            *slot = Some(thread::spawn(move || worker.schedule()));
        }
    }
    pub fn snapshot(&self) -> State {
        self.inner.state.lock().unwrap().clone()
    }
    pub fn notice(&self) -> String {
        std::mem::take(&mut *self.inner.notice.lock().unwrap())
    }

    fn persist(&self, state: &State) -> Result<()> {
        model::save_json(&self.inner.file, state)
    }
    fn save(&self, state: &State) {
        if let Err(e) = self.persist(state) {
            *self.inner.notice.lock().unwrap() =
                format!("Cannot save queue / Nelze uložit frontu: {e}");
        }
    }
    pub fn settings(&self, mut value: Settings) -> Result<()> {
        value.validate()?;
        fs::create_dir_all(&value.download_directory)?;
        let mut s = self.inner.state.lock().unwrap();
        value.queue_paused = s.settings.queue_paused;
        let old = s.settings.clone();
        s.settings = value;
        if let Err(e) = self.persist(&s) {
            s.settings = old;
            return Err(e);
        }
        Ok(())
    }
    pub fn add(
        &self,
        text: &str,
        format: &str,
        quality: i32,
        directory: &str,
        start: bool,
    ) -> Result<(usize, usize)> {
        let links: Vec<_> = regex::Regex::new(r#"(?i)https?://[^\s<>"']+"#)?
            .find_iter(text)
            .map(|m| model::parse_url(m.as_str().trim_end_matches([',', ';'])))
            .collect::<Result<_>>()?;
        if links.is_empty() || links.len() > 100 {
            bail!("Enter 1–100 HTTP(S) links / Vložte 1–100 odkazů HTTP(S)");
        }
        if !matches!(format, "MP4" | "MKV" | "MP3" | "Originál")
            || !model::QUALITIES.contains(&quality)
        {
            bail!("Invalid format or quality / Neplatný formát nebo kvalita");
        }
        if !Path::new(directory).is_absolute() {
            bail!("Choose an absolute destination path / Vyberte úplnou cestu cílové složky");
        }
        fs::create_dir_all(directory)?;
        for u in &links {
            model::canonical(u)?;
        }
        let mut s = self.inner.state.lock().unwrap();
        let before = s.jobs.clone();
        let (mut added, mut duplicate) = (0, 0);
        for u in links {
            let canonical = model::canonical(&u)?;
            let source = model::source(&u);
            let format = if source == "FastShare" {
                "Originál"
            } else {
                format
            };
            if s.jobs.iter().any(|j| {
                j.state != JobState::Cancelled
                    && j.format == format
                    && (source == "FastShare" || j.quality == quality)
                    && model::parse_url(&j.url)
                        .and_then(|u| model::canonical(&u))
                        .is_ok_and(|c| c == canonical)
            }) {
                duplicate += 1;
                continue;
            }
            let mut title = u
                .path_segments()
                .and_then(|mut p| p.next_back())
                .unwrap_or_default()
                .to_string();
            if title.len() < 3 || title == "watch" || title.ends_with(".php") {
                title = format!(
                    "{} · {source}",
                    model::tr(&s.settings.language, "Nový odkaz", "New link")
                );
            }
            s.jobs.push(Job {
                url: u.to_string(),
                title,
                source: source.into(),
                format: format.into(),
                quality,
                directory: directory.into(),
                state: if start {
                    JobState::Queued
                } else {
                    JobState::Paused
                },
                message: if start { "@queued" } else { "@paused" }.into(),
                ..Job::default()
            });
            added += 1;
        }
        if let Err(e) = self.persist(&s) {
            s.jobs = before;
            return Err(e);
        }
        Ok((added, duplicate))
    }
    pub fn add_presets(&self, links: &[String]) -> Result<usize> {
        let settings = self.snapshot().settings;
        let mut total = 0;
        for link in links {
            let u = model::parse_url(link)?;
            let p = settings.preference(model::source(&u));
            total += self
                .add(
                    link,
                    &p.format,
                    p.quality,
                    &settings.download_directory,
                    true,
                )?
                .0;
        }
        Ok(total)
    }
    pub fn change_state(&self, id: &str, to: JobState) {
        let mut s = self.inner.state.lock().unwrap();
        if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
            if j.state == JobState::Completed {
                return;
            }
            j.state = to;
            j.speed = 0.;
            j.finished = None;
            j.message = match to {
                JobState::Queued => "@queued",
                JobState::Paused => "@paused",
                _ => "@cancelled",
            }
            .into();
            if to != JobState::Queued {
                if let Some(flag) = self.inner.cancellations.lock().unwrap().get(id) {
                    flag.store(true, Ordering::Relaxed);
                }
            }
        }
        self.save(&s);
    }
    pub fn toggle_queue(&self) {
        let mut s = self.inner.state.lock().unwrap();
        s.settings.queue_paused = !s.settings.queue_paused;
        if s.settings.queue_paused {
            for j in &mut s.jobs {
                if j.state.active() {
                    j.state = JobState::Queued;
                    j.speed = 0.;
                    j.message = "@queued".into();
                    if let Some(flag) = self.inner.cancellations.lock().unwrap().get(&j.id) {
                        flag.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        self.save(&s);
    }
    pub fn prioritize(&self, id: &str) {
        let mut s = self.inner.state.lock().unwrap();
        if let Some(i) = s
            .jobs
            .iter()
            .position(|j| j.id == id && j.state == JobState::Queued)
        {
            let j = s.jobs.remove(i);
            s.jobs.insert(0, j);
            self.save(&s);
        }
    }
    pub fn remove_records(&self, ids: &HashSet<String>, library: bool) -> Result<usize> {
        let mut s = self.inner.state.lock().unwrap();
        let old = s.jobs.clone();
        let mut count = 0;
        let running = self.inner.cancellations.lock().unwrap();
        let files = self.inner.file_operations.lock().unwrap();
        for j in &mut s.jobs {
            if ids.contains(&j.id)
                && !j.state.active()
                && j.state != JobState::Queued
                && !running.contains_key(&j.id)
                && !files.contains(&j.file_path)
                && (if library {
                    j.in_library()
                } else {
                    !j.hidden_from_queue
                })
            {
                if library {
                    j.removed_from_library = true;
                } else {
                    j.hidden_from_queue = true;
                }
                count += 1;
            }
        }
        s.jobs.retain(|j| {
            !j.hidden_from_queue
                || j.in_library()
                || j.state.active()
                || j.state == JobState::Queued
                || running.contains_key(&j.id)
                || files.contains(&j.file_path)
        });
        if let Err(e) = self.persist(&s) {
            s.jobs = old;
            return Err(e);
        }
        Ok(count)
    }
    pub fn transfer(&self, id: &str, directory: &Path, moving: bool) -> Result<PathBuf> {
        if !directory.is_absolute() || !directory.is_dir() {
            bail!("Choose an existing destination folder / Vyberte existující cílovou složku");
        }
        let source = self.begin_file(id)?;
        let result = (|| {
            let target = directory.join(Path::new(&source).file_name().context("File name")?);
            let mut output=fs::OpenOptions::new().create_new(true).write(true).open(&target).context("Destination exists or is not writable; nothing overwritten / Cíl existuje nebo do něj nelze zapisovat; nic nepřepsáno")?;
            let copied = (|| {
                let mut input = fs::File::open(&source)?;
                std::io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                Ok::<_, anyhow::Error>(())
            })();
            drop(output);
            if let Err(e) = copied {
                let _ = fs::remove_file(&target);
                return Err(e);
            }
            if moving {
                let mut s = self.inner.state.lock().unwrap();
                let old = s.jobs.clone();
                for j in &mut s.jobs {
                    if j.file_path == source {
                        j.file_path = target.to_string_lossy().into();
                        j.directory = directory.to_string_lossy().into();
                    }
                }
                if let Err(e) = self.persist(&s) {
                    s.jobs = old;
                    let _ = fs::remove_file(&target);
                    return Err(e);
                }
                if let Err(e) = fs::remove_file(&source) {
                    bail!(
                        "New copy saved; original could not be removed / Nová kopie uložena; původní soubor nelze odstranit: {e}"
                    );
                }
            }
            Ok(target)
        })();
        self.inner.file_operations.lock().unwrap().remove(&source);
        result
    }
    fn begin_file(&self, id: &str) -> Result<String> {
        let s = self.inner.state.lock().unwrap();
        let j = s
            .jobs
            .iter()
            .find(|j| j.id == id && j.state == JobState::Completed)
            .context("File is not completed / Soubor není dokončený")?;
        if !Path::new(&j.file_path).is_file() {
            bail!("File is no longer at its saved location / Soubor už v uloženém umístění není");
        }
        if !self
            .inner
            .file_operations
            .lock()
            .unwrap()
            .insert(j.file_path.clone())
        {
            bail!("A file operation is already running / Se souborem právě probíhá jiná operace");
        }
        Ok(j.file_path.clone())
    }
    pub fn recycle(&self, id: &str) -> Result<()> {
        let path = self.begin_file(id)?;
        let result = trash::delete(&path).map_err(anyhow::Error::from);
        self.inner.file_operations.lock().unwrap().remove(&path);
        result?;
        let mut s = self.inner.state.lock().unwrap();
        for j in &mut s.jobs {
            if j.file_path == path {
                j.message = "File moved to recycle bin / Soubor přesunut do koše".into();
            }
        }
        self.save(&s);
        Ok(())
    }
    fn change(&self, id: &str, f: impl FnOnce(&mut Job)) {
        let mut s = self.inner.state.lock().unwrap();
        if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id && j.state.active()) {
            f(j);
        }
    }
    fn schedule(&self) {
        let mut workers: HashMap<String, thread::JoinHandle<()>> = HashMap::new();
        let mut saved = Instant::now();
        while !self.inner.stopping.load(Ordering::Relaxed) {
            let done: Vec<_> = workers
                .iter()
                .filter(|(_, h)| h.is_finished())
                .map(|(id, _)| id.clone())
                .collect();
            for id in done {
                if let Some(h) = workers.remove(&id) {
                    if h.join().is_err() {
                        self.change(&id,|j|{j.state=JobState::Failed;j.message="Worker stopped unexpectedly / Stahovací proces se neočekávaně zastavil".into();});
                    }
                }
                self.inner.cancellations.lock().unwrap().remove(&id);
            }
            let mut s = self.inner.state.lock().unwrap();
            let settings = s.settings.clone();
            if !settings.queue_paused {
                for j in &mut s.jobs {
                    if workers.len() >= settings.parallel {
                        break;
                    }
                    if j.state != JobState::Queued || workers.contains_key(&j.id) {
                        continue;
                    }
                    if j.source == "FastShare"
                        && !self.fast.logged_in()
                        && workers.keys().any(|id| {
                            self.inner
                                .cancellations
                                .lock()
                                .unwrap()
                                .contains_key(&format!("fs:{id}"))
                        })
                    {
                        continue;
                    }
                    j.state = JobState::Connecting;
                    j.message = "@connecting".into();
                    let job = j.clone();
                    let settings = settings.clone();
                    let worker = self.clone();
                    let cancel = Arc::new(AtomicBool::new(false));
                    self.inner
                        .cancellations
                        .lock()
                        .unwrap()
                        .insert(j.id.clone(), cancel.clone());
                    if j.source == "FastShare" {
                        self.inner
                            .cancellations
                            .lock()
                            .unwrap()
                            .insert(format!("fs:{}", j.id), cancel.clone());
                    }
                    let id = j.id.clone();
                    workers.insert(
                        id,
                        thread::spawn(move || {
                            let result = if job.source == "FastShare" {
                                worker.download_fast(&job, &cancel)
                            } else {
                                worker.download_video(&job, &settings, &cancel)
                            };
                            if !cancel.load(Ordering::Relaxed) {
                                match result {
                                    Ok(()) => worker.change(&job.id, |j| {
                                        j.state = JobState::Completed;
                                        j.percent = 100.;
                                        j.speed = 0.;
                                        j.message = "@completed".into();
                                        j.finished = Some(model::now());
                                    }),
                                    Err(e) => worker.change(&job.id, |j| {
                                        j.state = JobState::Failed;
                                        j.speed = 0.;
                                        j.message =
                                            sanitize_log(&format!("{e:#}"), &settings.cookies_file);
                                    }),
                                }
                            }
                            worker
                                .inner
                                .cancellations
                                .lock()
                                .unwrap()
                                .remove(&format!("fs:{}", job.id));
                            let state = worker.inner.state.lock().unwrap();
                            worker.save(&state);
                        }),
                    );
                }
            }
            if saved.elapsed() > Duration::from_secs(2) {
                self.save(&s);
                saved = Instant::now();
            }
            drop(s);
            thread::sleep(Duration::from_millis(100));
        }
        for flag in self.inner.cancellations.lock().unwrap().values() {
            flag.store(true, Ordering::Relaxed);
        }
        for (_, h) in workers {
            let _ = h.join();
        }
        let s = self.inner.state.lock().unwrap();
        self.save(&s);
    }
    pub fn shutdown(&self) {
        if self.inner.stopping.swap(true, Ordering::Relaxed) {
            return;
        }
        let mut s = self.inner.state.lock().unwrap();
        for j in &mut s.jobs {
            if j.state.active() {
                j.state = JobState::Paused;
                j.speed = 0.;
                j.message = "@interrupted".into();
            }
        }
        for f in self.inner.cancellations.lock().unwrap().values() {
            f.store(true, Ordering::Relaxed);
        }
        self.save(&s);
        drop(s);
        if let Some(h) = self.inner.scheduler.lock().unwrap().take() {
            let _ = h.join();
        }
    }
    fn download_video(&self, job: &Job, settings: &Settings, cancel: &AtomicBool) -> Result<()> {
        let path = crate::media::download(job, settings, cancel, |update| {
            self.change(&job.id, |j| match update {
                crate::media::Update::Title(title) => j.title = title,
                crate::media::Update::CookieProfile {
                    index,
                    total,
                    label,
                } => {
                    j.state = JobState::Connecting;
                    j.speed = 0.;
                    j.downloaded = 0;
                    j.total = 0;
                    j.percent = 0.;
                    j.message = format!("@cookie-profile:{index}/{total} · {label}");
                }
                crate::media::Update::Processing => {
                    j.state = JobState::Processing;
                    j.speed = 0.;
                    j.message = "@processing".into();
                }
                crate::media::Update::Progress { done, total, speed } => {
                    j.state = JobState::Downloading;
                    j.downloaded = done;
                    j.total = total;
                    j.speed = speed;
                    j.percent = if total > 0 {
                        (100. * done as f64 / total as f64).min(99.9)
                    } else {
                        0.
                    };
                    j.message = format!(
                        "{} / {}",
                        model::bytes(done as f64),
                        model::bytes(total as f64)
                    );
                }
            })
        })?;
        let size = fs::metadata(&path)?.len();
        self.change(&job.id, |j| {
            j.file_path = path.to_string_lossy().into();
            j.downloaded = size;
            j.total = size;
        });
        Ok(())
    }
    fn download_fast(&self, job: &Job, cancel: &AtomicBool) -> Result<()> {
        fs::create_dir_all(&job.directory)?;
        let partial_dir = Path::new(&job.directory).join(".workflow-partials");
        fs::create_dir_all(&partial_dir)?;
        let partial = partial_dir.join(format!("{}.part", model::safe_name(&job.id)));
        let mut offset = fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
        let (mut response, name) = self.fast.open(job, offset)?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let resumed = response.status().as_u16() == 206;
        let range = response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let (range_start, range_total) = parse_range(&range);
        if resumed && (offset == 0 || range_start != Some(offset)) {
            bail!(
                "Invalid server range; partial file preserved / Neplatný rozsah dat; rozpracovaný soubor zachován"
            );
        }
        if !resumed {
            offset = 0;
        }
        let total = range_total
            .or_else(|| response.content_length().map(|n| n + offset))
            .unwrap_or(0);
        if total > offset
            && fs2::available_space(&job.directory).unwrap_or(u64::MAX)
                < total - offset + 64 * 1024 * 1024
        {
            bail!("Not enough disk space / Nedostatek místa na disku");
        }
        let validator = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .filter(|v| !v.starts_with("W/"))
            .or_else(|| {
                response
                    .headers()
                    .get("last-modified")
                    .and_then(|v| v.to_str().ok())
            })
            .unwrap_or("")
            .to_string();
        let name = response
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(disposition_name)
            .unwrap_or(name);
        let name = model::safe_name(&name);
        let p = Path::new(&name);
        let ext = p
            .extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        let id = model::file_id(&model::parse_url(&job.url)?)?;
        let mut target = Path::new(&job.directory).join(format!("{stem} [FS-{id}]{ext}"));
        if target.exists() {
            target = Path::new(&job.directory).join(format!(
                "{stem} [FS-{id}]-{}{ext}",
                job.id.chars().take(8).collect::<String>()
            ));
        }
        self.change(&job.id, |j| {
            j.title = name;
            j.state = JobState::Downloading;
            j.total = total;
            j.downloaded = offset;
            j.validator = validator;
        });
        let mut output = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&partial)?;
        let mut buffer = [0u8; 131072];
        let mut downloaded = offset;
        let (mut previous, mut last) = (offset, Instant::now());
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let n = response.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n])?;
            downloaded += n as u64;
            if last.elapsed() > Duration::from_millis(200) {
                let speed = (downloaded - previous) as f64 / last.elapsed().as_secs_f64();
                previous = downloaded;
                last = Instant::now();
                self.change(&job.id, |j| {
                    j.downloaded = downloaded;
                    j.speed = speed;
                    j.percent = if total > 0 {
                        (100. * downloaded as f64 / total as f64).min(99.9)
                    } else {
                        0.
                    };
                    j.message = format!(
                        "{} / {}",
                        model::bytes(downloaded as f64),
                        model::bytes(total as f64)
                    );
                });
            }
        }
        output.sync_all()?;
        drop(output);
        if downloaded == 0 || (total > 0 && downloaded != total) {
            bail!(
                "Incomplete transfer. Resume to continue. / Neúplný přenos. Pokračujte tlačítkem Obnovit."
            );
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        // Reserve the final name first: never overwrite a file created during transfer.
        let mut final_output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)?;
        let result = (|| {
            std::io::copy(&mut fs::File::open(&partial)?, &mut final_output)?;
            final_output.sync_all()?;
            Ok::<_, anyhow::Error>(())
        })();
        drop(final_output);
        if let Err(e) = result {
            let _ = fs::remove_file(&target);
            return Err(e);
        }
        fs::remove_file(partial)?;
        self.change(&job.id, |j| {
            j.file_path = target.to_string_lossy().into();
            j.downloaded = downloaded;
            j.total = downloaded;
        });
        Ok(())
    }
}
pub fn sanitize_log(line: &str, cookies: &str) -> String {
    let clean = regex::Regex::new(r"https?://\S+")
        .unwrap()
        .replace_all(line, "[URL]")
        .to_string();
    if cookies.is_empty() {
        clean
    } else {
        clean.replace(cookies, "[cookies]")
    }
}
pub fn parse_range(text: &str) -> (Option<u64>, Option<u64>) {
    let Some(text) = text.strip_prefix("bytes ") else {
        return (None, None);
    };
    let Some((range, total)) = text.split_once('/') else {
        return (None, None);
    };
    let start = range.split_once('-').and_then(|(s, _)| s.parse().ok());
    (start, total.parse().ok())
}
fn disposition_name(text: &str) -> Option<String> {
    for part in text.split(';') {
        if let Some(value) = part.trim().strip_prefix("filename*=UTF-8''") {
            let u = url::Url::parse(&format!("https://example.invalid/?name={value}")).ok()?;
            return u.query_pairs().next().map(|(_, v)| v.into_owned());
        }
    }
    text.split(';').find_map(|p| {
        p.trim()
            .strip_prefix("filename=")
            .map(|s| s.trim_matches('"').to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_ranges() {
        assert_eq!(parse_range("bytes 200-999/1000"), (Some(200), Some(1000)));
        assert_eq!(parse_range("nonsense"), (None, None));
    }
    #[test]
    fn logs_hide_urls() {
        assert!(
            !sanitize_log(
                "ERROR https://example.com/?token=secret cookie.txt",
                "cookie.txt"
            )
            .contains("secret")
        );
    }
    #[test]
    fn queue_library_independence() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = State::default();
        state.settings.queue_paused = true;
        state.jobs.push(Job {
            id: "item".into(),
            state: JobState::Completed,
            ..Job::default()
        });
        model::save_json(&dir.path().join("queue.json"), &state).unwrap();
        let engine = Engine::new(dir.path(), None).unwrap();
        let ids = HashSet::from(["item".into()]);
        engine.remove_records(&ids, false).unwrap();
        let s = engine.snapshot();
        assert!(s.jobs[0].hidden_from_queue);
        assert!(s.jobs[0].in_library());
        engine.remove_records(&ids, true).unwrap();
        assert!(engine.snapshot().jobs.is_empty());
        engine.shutdown();
    }
    #[test]
    fn copy_move_preserve_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("file.bin");
        fs::write(&src, b"preserve every byte").unwrap();
        let dest = dir.path().join("dest");
        fs::create_dir(&dest).unwrap();
        let mut state = State::default();
        state.settings.queue_paused = true;
        state.jobs.push(Job {
            id: "item".into(),
            state: JobState::Completed,
            file_path: src.to_string_lossy().into(),
            ..Job::default()
        });
        model::save_json(&dir.path().join("queue.json"), &state).unwrap();
        let e = Engine::new(dir.path(), None).unwrap();
        let target = e.transfer("item", &dest, false).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"preserve every byte");
        assert!(e.transfer("item", &dest, true).is_err());
        assert!(src.exists());
        fs::remove_file(target).unwrap();
        let target = e.transfer("item", &dest, true).unwrap();
        assert!(!src.exists());
        assert_eq!(e.snapshot().jobs[0].file_path, target.to_string_lossy());
        e.shutdown();
    }
}
