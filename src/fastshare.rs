use crate::model::{self, Job};
use anyhow::{Context, Result, bail};
use base64::Engine;
use reqwest::{
    blocking::{Client, Response},
    header::{ACCEPT_ENCODING, CONTENT_TYPE, COOKIE, IF_RANGE, LOCATION, RANGE},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Session {
    pub token: String,
    pub username: String,
    pub unlimited: bool,
    pub credit: String,
    #[serde(default)]
    password: String,
}
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub resolution: String,
}
#[derive(Clone)]
pub struct FastShare {
    path: PathBuf,
    session: Arc<Mutex<Option<Session>>>,
}
impl FastShare {
    pub fn new(data: &Path) -> Self {
        let path = data.join("fastshare.session");
        let session = crate::platform::load_secret(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        Self {
            path,
            session: Arc::new(Mutex::new(session)),
        }
    }
    pub fn logged_in(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }
    pub fn password_saved(&self) -> bool {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| !s.password.is_empty())
    }
    pub fn renew_login(&self) -> Result<()> {
        let saved = self
            .session
            .lock()
            .unwrap()
            .clone()
            .context("No saved login / Přihlášení není uloženo")?;
        if saved.password.is_empty() {
            bail!(
                "Sign in once to remember your password / Pro zapamatování hesla se jednou přihlaste"
            );
        }
        self.login(&saved.username, &saved.password)
    }
    pub fn account(&self, lang: &str) -> String {
        self.session
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| {
                format!(
                    "{} · {}",
                    s.username,
                    if s.unlimited {
                        model::tr(lang, "neomezený účet", "unlimited").to_owned()
                    } else {
                        format!("{} {} MB", model::tr(lang, "kredit", "credit"), s.credit)
                    }
                )
            })
            .unwrap_or_else(|| {
                model::tr(
                    lang,
                    "Bez účtu · 1 přenos FastShare",
                    "Not signed in · 1 FastShare transfer",
                )
                .into()
            })
    }
    pub fn logout(&self) -> Result<()> {
        crate::platform::delete_secret(&self.path)?;
        *self.session.lock().unwrap() = None;
        Ok(())
    }
    pub fn login(&self, username: &str, password: &str) -> Result<()> {
        if username.trim().is_empty() || password.is_empty() {
            bail!("Enter username and password / Vyplňte jméno a heslo");
        }
        let data = api(&[
            ("process", "login"),
            ("login", username.trim()),
            ("password", password),
        ])?;
        let user = &data["user"];
        let token = value_string(&user["hash"]);
        if !(10..=512).contains(&token.len())
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            bail!(
                "Login failed. Check your credentials. / Přihlášení se nezdařilo. Zkontrolujte údaje."
            );
        }
        let next = Session {
            token,
            username: username.trim().into(),
            unlimited: matches!(value_string(&user["unlimited"]).as_str(), "1" | "true"),
            credit: value_string(&user["data"]["value"]),
            password: password.to_owned(),
        };
        crate::platform::save_secret(&self.path, &serde_json::to_vec(&next)?)?;
        *self.session.lock().unwrap() = Some(next);
        Ok(())
    }
    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        if !(2..=160).contains(&query.trim().chars().count()) {
            bail!("Search needs 2–160 characters / Hledání vyžaduje 2–160 znaků");
        }
        let data = api(&[
            ("process", "search"),
            ("pagination", "60"),
            ("term", query.trim()),
            ("adult", "false"),
        ])?;
        let rows = &data["search"]["file"];
        let rows = if let Some(a) = rows.as_array() {
            a.clone()
        } else if rows.is_object() {
            vec![rows.clone()]
        } else {
            vec![]
        };
        Ok(rows
            .iter()
            .take(60)
            .filter_map(|row| {
                let u = safe_uri(&value_string(&row["download_url"]), None).ok()?;
                model::file_id(&u).ok()?;
                Some(SearchResult {
                    name: value_string(&row["filename"]),
                    url: u.to_string(),
                    size: value_string(&row["data"]["value"]).parse().unwrap_or(0),
                    resolution: value_string(&row["resolution"]),
                })
            })
            .collect())
    }
    pub fn open(&self, job: &Job, offset: u64) -> Result<(Response, String)> {
        match self.open_once(job, offset) {
            Err(e)
                if self.password_saved()
                    && e.downcast_ref::<reqwest::Error>().is_some_and(|e| {
                        e.status()
                            .is_some_and(|s| s.as_u16() == 401 || s.as_u16() == 403)
                    }) =>
            {
                self.renew_login()?;
                self.open_once(job, offset)
            }
            result => result,
        }
    }
    fn open_once(&self, job: &Job, offset: u64) -> Result<(Response, String)> {
        let token = self
            .session
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.token.clone())
            .unwrap_or_default();
        let client = client()?;
        let mut uri = safe_uri(&job.url, None)?;
        let id = model::file_id(&uri)?;
        let mut name = job.title.clone();
        uri.set_query(if uri.path() == "/download.php" {
            Some("")
        } else {
            None
        });
        if uri.path() == "/download.php" {
            uri.query_pairs_mut().append_pair("id", &id);
        }
        if uri.path() == "/download.php" {
            if token.is_empty() {
                let mut free = Url::parse("https://fastshare.cloud/free/")?;
                free.query_pairs_mut()
                    .append_pair("lang", "cs")
                    .append_pair("u", &id)
                    .append_pair(
                        "r",
                        &base64::engine::general_purpose::STANDARD.encode(uri.as_str()),
                    );
                return Ok((
                    check(request(
                        &client,
                        &token,
                        free,
                        true,
                        offset,
                        &job.validator,
                        true,
                    )?)?,
                    name,
                ));
            }
            return Ok((
                check(request(
                    &client,
                    &token,
                    uri,
                    false,
                    offset,
                    &job.validator,
                    true,
                )?)?,
                name,
            ));
        }
        for _ in 0..4 {
            let response = request(&client, &token, uri.clone(), false, 0, "", true)?;
            if !is_page(&response) {
                return Ok((response, name));
            }
            let body = text(response)?;
            if let Some(cap) = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>")?.captures(&body)
            {
                name = html_escape::decode_html_entities(&cap[1])
                    .split('|')
                    .next()
                    .unwrap_or_default()
                    .split(" online ke ")
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .into();
            }
            let mut refresh = None;
            for cap in regex::Regex::new(r"(?is)<meta\b[^>]*>")?.find_iter(&body) {
                let tag = cap.as_str();
                if attribute(tag, "http-equiv").eq_ignore_ascii_case("refresh") {
                    let content = attribute(tag, "content");
                    if let Some(c) = regex::Regex::new(r"(?i)url\s*=\s*(.+)")?.captures(&content) {
                        refresh = Some(c[1].trim_matches([' ', '\'', '"']).to_string());
                    }
                }
            }
            if let Some(target) = refresh {
                uri = safe_uri(&target, Some(&uri))?;
                continue;
            }
            if token.is_empty() {
                let form=regex::Regex::new(r"(?is)<form\b[^>]*>")?.find_iter(&body).map(|m|attribute(m.as_str(),"action")).find(|a|a.starts_with("/free/")).context("Free download unavailable. Open the source or sign in. / Bezplatné stahování není dostupné. Otevřete zdroj nebo se přihlaste.")?;
                return Ok((
                    check(request(
                        &client,
                        &token,
                        safe_uri(&form, Some(&uri))?,
                        true,
                        offset,
                        &job.validator,
                        true,
                    )?)?,
                    name,
                ));
            }
            for link in regex::Regex::new(r"(?is)<a\b[^>]*>")?.find_iter(&body) {
                let href = attribute(link.as_str(), "href");
                if !href.contains("/download.php?") {
                    continue;
                }
                let target = safe_uri(&href, Some(&uri))?;
                if model::file_id(&target)? == id {
                    return Ok((
                        check(request(
                            &client,
                            &token,
                            target,
                            false,
                            offset,
                            &job.validator,
                            true,
                        )?)?,
                        name,
                    ));
                }
            }
            bail!(
                "FastShare did not provide a download link / FastShare neposkytl odkaz ke stažení"
            );
        }
        bail!("Too many file page redirects / Příliš mnoho přesměrování stránky souboru")
    }
}
fn value_string(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        s.into()
    } else if v.is_null() {
        String::new()
    } else {
        v.to_string()
    }
}
pub fn safe_uri(raw: &str, base: Option<&Url>) -> Result<Url> {
    let raw = html_escape::decode_html_entities(raw);
    let mut u = if let Some(base) = base {
        base.join(&raw)?
    } else {
        model::parse_url(&raw)?
    };
    if !model::is_fastshare(&u)
        || !matches!(u.scheme(), "http" | "https")
        || u.port().is_some()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        bail!(
            "FastShare returned an untrusted server address / FastShare vrátil nepovolenou adresu serveru"
        );
    }
    u.set_scheme("https").ok();
    u.set_fragment(None);
    Ok(u)
}
fn client() -> Result<Client> {
    Ok(Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(35))
        .user_agent("WorkFlow-Media/0.5")
        .build()?)
}
fn request(
    client: &Client,
    token: &str,
    mut uri: Url,
    mut post: bool,
    offset: u64,
    validator: &str,
    redirects: bool,
) -> Result<Response> {
    for _ in 0..7 {
        uri = safe_uri(uri.as_str(), None)?;
        let mut req = if post {
            client.post(uri.clone()).body("")
        } else {
            client.get(uri.clone())
        };
        req = req.header(ACCEPT_ENCODING, "identity");
        if !token.is_empty() {
            req = req.header(COOKIE, format!("FASTSHARE={token}"));
        }
        if offset > 0 && !validator.is_empty() {
            req = req
                .header(RANGE, format!("bytes={offset}-"))
                .header(IF_RANGE, validator);
        }
        let response = req.send().map_err(|_| {
            anyhow::anyhow!("FastShare connection failed / Připojení k FastShare selhalo")
        })?;
        if response.status().is_redirection() {
            if !redirects {
                bail!("Unexpected API redirect / Neočekávané přesměrování API");
            }
            let location = response
                .headers()
                .get(LOCATION)
                .context("Missing redirect")?
                .to_str()?;
            uri = safe_uri(location, Some(&uri))?;
            post = false;
            continue;
        }
        if !response.status().is_success() {
            bail!(
                "FastShare HTTP {}: check availability, account and limits / zkontrolujte dostupnost, účet a limity",
                response.status().as_u16()
            );
        }
        return Ok(response);
    }
    bail!("Too many redirects / Příliš mnoho přesměrování")
}
fn text(response: Response) -> Result<String> {
    let mut body = String::new();
    response.take(2_000_001).read_to_string(&mut body)?;
    if body.len() > 2_000_000 {
        bail!("Response too large / Příliš velká odpověď");
    }
    Ok(body)
}
fn api(parameters: &[(&str, &str)]) -> Result<Value> {
    let result = (|| {
        let mut uri = Url::parse("https://fastshare.cz/api/api_kodi.php")?;
        uri.query_pairs_mut()
            .extend_pairs(parameters.iter().copied());
        let response = request(&client()?, "", uri, false, 0, "", false)?;
        Ok::<_, anyhow::Error>(serde_json::from_str(&text(response)?)?)
    })();
    result.map_err(|_|anyhow::anyhow!("FastShare request failed. Check connection or credentials. / FastShare požadavek nepřijal. Zkontrolujte připojení nebo přihlašovací údaje."))
}
fn attribute(tag: &str, name: &str) -> String {
    regex::Regex::new(&format!(
        r#"(?i)\b{}\s*=\s*["']([^"']*)["']"#,
        regex::escape(name)
    ))
    .ok()
    .and_then(|r| {
        r.captures(tag)
            .map(|c| html_escape::decode_html_entities(&c[1]).into_owned())
    })
    .unwrap_or_default()
}
fn is_page(r: &Response) -> bool {
    let t = r
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    t.starts_with("text/") || t.contains("json") || t.contains("xhtml")
}
fn check(r: Response) -> Result<Response> {
    if !is_page(&r) {
        return Ok(r);
    }
    let body = text(r)?.to_ascii_lowercase();
    if body.contains("captcha") {
        bail!(
            "FastShare requires CAPTCHA. Open the source in your browser. / FastShare požaduje CAPTCHA. Otevřete zdroj v prohlížeči."
        );
    }
    bail!(
        "FastShare returned a page instead of a file. Check credit and limits. / FastShare vrátil stránku místo souboru. Zkontrolujte kredit a limity."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn saved_password_is_encrypted_and_restored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fastshare.session");
        let saved = Session {
            token: "synthetic-session-token".into(),
            username: "test-user".into(),
            password: "synthetic-password-only".into(),
            unlimited: false,
            credit: "0".into(),
        };
        crate::platform::save_secret(&path, &serde_json::to_vec(&saved).unwrap()).unwrap();
        let encrypted = std::fs::read(&path).unwrap();
        assert!(
            !encrypted
                .windows(saved.password.len())
                .any(|w| w == saved.password.as_bytes())
        );
        let restored = FastShare::new(directory.path());
        crate::platform::save_secret(&path, &serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(restored.password_saved());
        assert_eq!(
            restored.session.lock().unwrap().as_ref().unwrap().password,
            saved.password
        );
        restored.logout().unwrap();
        assert!(!directory.path().join("fastshare.session.bak").exists());
        assert!(!FastShare::new(directory.path()).password_saved());
    }
    #[test]
    fn redirects_are_scoped() {
        assert!(safe_uri("https://evil.test/1", None).is_err());
        assert!(safe_uri("https://fastshare.cz:8443/1", None).is_err());
        assert!(
            safe_uri(
                "/free/?u=1&amp;x=2",
                Some(&Url::parse("https://fastshare.cz/1/file").unwrap())
            )
            .unwrap()
            .as_str()
            .contains("&x=2")
        );
    }
    #[test]
    fn attributes() {
        assert_eq!(
            attribute("<form action='/free/?a=1&amp;b=2'>", "action"),
            "/free/?a=1&b=2"
        );
    }
}
