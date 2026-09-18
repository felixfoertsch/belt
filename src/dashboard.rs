use std::fs;
use std::io::{BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://dashboard.uberspace.de";

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardAsteroid {
    pub name: String,
    pub hostname: String,
    pub version: u8,
    pub created: String,
    pub storage: String,
    pub balance: String,
    pub price: String,
}

pub fn login(login: &str, password_stdin: bool) -> Result<(), String> {
    if login.trim().is_empty() {
        return Err("login must not be empty".into());
    }
    let password = if password_stdin {
        let mut password = String::new();
        std::io::stdin()
            .read_to_string(&mut password)
            .map_err(|e| format!("failed to read password from stdin: {e}"))?;
        password.trim_end_matches(['\r', '\n']).to_string()
    } else {
        rpassword::prompt_password("Uberspace password: ")
            .map_err(|e| format!("failed to read password: {e}"))?
    };
    if password.is_empty() {
        return Err("password must not be empty".into());
    }

    let store = Arc::new(CookieStoreMutex::new(CookieStore::new()));
    let client = client(Arc::clone(&store))?;
    let response = client
        .get(format!("{BASE_URL}/login?lang=en"))
        .send()
        .map_err(|e| format!("failed to load login page: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("login page returned HTTP {status}"));
    }
    let csrf = csrf_token(
        &response
            .text()
            .map_err(|e| format!("failed to read login page: {e}"))?,
    )?;
    let response = client
        .post(format!("{BASE_URL}/login/validate"))
        .form(&[
            ("_csrf_token", csrf.as_str()),
            ("login", login),
            ("password", password.as_str()),
        ])
        .send()
        .map_err(|e| format!("login request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("login returned HTTP {status}"));
    }
    match response.url().path() {
        "/meta" => save_session(&store),
        "/login/secondfactor" => {
            Err("account requires a second factor; dashboard WebAuthn is not supported yet".into())
        }
        _ => Err("login failed; check login and password".into()),
    }
}

pub fn logout() -> Result<(), String> {
    let (client, _) = authenticated_client()?;
    let response = client
        .get(format!("{BASE_URL}/logout"))
        .send()
        .map_err(|e| format!("logout request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("logout returned HTTP {}", response.status()));
    }
    remove_session()
}

pub fn list() -> Result<Vec<DashboardAsteroid>, String> {
    let (client, _) = authenticated_client()?;
    let response = client
        .get(format!("{BASE_URL}/meta"))
        .send()
        .map_err(|e| format!("dashboard request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("dashboard returned HTTP {}", response.status()));
    }
    if response.url().path().starts_with("/login") {
        return Err("dashboard session expired; run `belt login <username>`".into());
    }
    parse_asteroids(
        &response
            .text()
            .map_err(|e| format!("failed to read dashboard: {e}"))?,
    )
}

fn client(store: Arc<CookieStoreMutex>) -> Result<Client, String> {
    Client::builder()
        .cookie_provider(store)
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("belt/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))
}

fn authenticated_client() -> Result<(Client, Arc<CookieStoreMutex>), String> {
    let path = session_path()?;
    let file = fs::File::open(&path)
        .map_err(|_| "not logged in; run `belt login <username>`".to_string())?;
    let store = cookie_store::serde::json::load(BufReader::new(file))
        .map_err(|e| format!("failed to read dashboard session: {e}"))?;
    let store = Arc::new(CookieStoreMutex::new(store));
    Ok((client(Arc::clone(&store))?, store))
}

fn csrf_token(html: &str) -> Result<String, String> {
    let document = Html::parse_document(html);
    let selector =
        Selector::parse("form[action$='login/validate'] input[name='_csrf_token']").unwrap();
    document
        .select(&selector)
        .next()
        .and_then(|element| element.value().attr("value"))
        .map(str::to_string)
        .ok_or_else(|| "login page did not contain a CSRF token".to_string())
}

fn parse_asteroids(html: &str) -> Result<Vec<DashboardAsteroid>, String> {
    let document = Html::parse_document(html);
    let row = Selector::parse("table tr").unwrap();
    let cell = Selector::parse("td").unwrap();
    let mut asteroids = Vec::new();
    for element in document.select(&row) {
        let values: Vec<String> = element
            .select(&cell)
            .map(|cell| {
                cell.text()
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        if values.len() >= 6 {
            let (hostname, version) = parse_host(&values[1])?;
            asteroids.push(DashboardAsteroid {
                name: values[0].clone(),
                hostname,
                version,
                created: values[2].clone(),
                storage: values[3].clone(),
                balance: values[4].clone(),
                price: values[5].clone(),
            });
        }
    }
    if asteroids.is_empty() {
        return Err("dashboard response did not contain an asteroid table".into());
    }
    Ok(asteroids)
}

fn parse_host(value: &str) -> Result<(String, u8), String> {
    let (hostname, generation) = value
        .rsplit_once(" (U")
        .and_then(|(hostname, generation)| generation.strip_suffix(')').map(|g| (hostname, g)))
        .ok_or_else(|| format!("dashboard returned invalid host: {value}"))?;
    let version = generation
        .parse()
        .map_err(|_| format!("dashboard returned invalid Uberspace version: {value}"))?;
    if version != 7 && version != 8 {
        return Err(format!(
            "dashboard returned unsupported Uberspace version: U{version}"
        ));
    }
    Ok((hostname.to_string(), version))
}

fn session_path() -> Result<PathBuf, String> {
    let config =
        dirs::config_dir().ok_or_else(|| "cannot determine config directory".to_string())?;
    Ok(config.join("belt").join("dashboard-session.json"))
}

fn save_session(store: &CookieStoreMutex) -> Result<(), String> {
    let path = session_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "session path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("failed to create config directory: {e}"))?;
    let temp = parent.join(format!(".dashboard-session.{}.tmp", std::process::id()));
    let file = create_private(&temp)?;
    let mut writer = BufWriter::new(file);
    let store = store
        .lock()
        .map_err(|_| "dashboard session lock poisoned".to_string())?;
    cookie_store::serde::json::save(&store, &mut writer)
        .map_err(|e| format!("failed to save dashboard session: {e}"))?;
    drop(writer);
    fs::rename(&temp, &path).map_err(|e| format!("failed to replace dashboard session: {e}"))
}

#[cfg(unix)]
fn create_private(path: &Path) -> Result<fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("failed to create dashboard session: {e}"))
}

#[cfg(not(unix))]
fn create_private(path: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("failed to create dashboard session: {e}"))
}

fn remove_session() -> Result<(), String> {
    let path = session_path()?;
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("failed to remove dashboard session: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csrf_and_asteroid_rows() {
        let login =
            r#"<form action="login/validate"><input name="_csrf_token" value="abc"></form>"#;
        assert_eq!(csrf_token(login).unwrap(), "abc");
        let meta = r#"<table><tr><th>Username</th></tr><tr><td>olympus</td><td>janus (U8)</td><td>2026-03-06</td><td>10 GB</td><td>10,00 €</td><td>2,00 €</td><td>Transfer</td></tr></table>"#;
        assert_eq!(
            parse_asteroids(meta).unwrap(),
            vec![DashboardAsteroid {
                name: "olympus".into(),
                hostname: "janus".into(),
                version: 8,
                created: "2026-03-06".into(),
                storage: "10 GB".into(),
                balance: "10,00 €".into(),
                price: "2,00 €".into(),
            }]
        );
    }
}
