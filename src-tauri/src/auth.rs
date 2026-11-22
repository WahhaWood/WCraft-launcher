//! WCraft accounts (backend/auth): the server is in offline mode, so it lets a name in only after the
//! launcher logged in to that account and said it is joining (`join`). Tokens are kept per name in
//! `auth.json` in the launcher's folder.

use crate::config;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;
use std::time::{Duration, Instant};

type Tokens = BTreeMap<String, String>;

fn tokens() -> Tokens {
    fs::read(config::data_dir().join("auth.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(tokens: &Tokens) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(tokens).map_err(|e| e.to_string())?;
    fs::write(config::data_dir().join("auth.json"), json).map_err(|e| e.to_string())
}

/// The token `name` is logged in with here.
pub fn token(name: &str) -> Option<String> {
    tokens().remove(name)
}

/// An endpoint of the accounts service, or of `WCRAFT_AUTH_URL` (a local service, tests).
pub fn url(path: &str) -> String {
    let base = std::env::var("WCRAFT_AUTH_URL").unwrap_or_else(|_| base());
    format!("{}{path}", base.trim_end_matches('/'))
}

static CACHE: Mutex<Option<(Instant, String)>> = Mutex::new(None);

/// The service's address now, asked at most every 5 minutes: the first of these that answers `/launcher/latest.json`
/// (4 s each): the published one (`endpoint`) and the mirrors it names, the last one that worked (kept on disk), the
/// pack's `auth` and its `mirrors`. None answering, the first one: the request then fails as it would have.
fn base() -> String {
    if let Some((at, url)) = &*CACHE.lock().unwrap() {
        if at.elapsed() < Duration::from_secs(300) {
            return url.clone();
        }
    }
    let saved = config::data_dir().join("auth-endpoint.txt");
    let pack = config::pack();
    let mut candidates: Vec<String> = Vec::new();
    if let Some((auth, mirrors)) = published() {
        candidates.push(auth);
        candidates.extend(mirrors);
    }
    candidates.extend(fs::read_to_string(&saved).ok().map(|s| s.trim().to_owned()).filter(|s| s.starts_with("https://")));
    candidates.push(pack.auth);
    candidates.extend(pack.mirrors);
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert(c.trim_end_matches('/').to_owned()));
    let url = match candidates.iter().find(|c| alive(c)) {
        Some(url) => {
            drop(fs::write(&saved, url));
            url.clone()
        }
        None => candidates[0].clone(),
    };
    *CACHE.lock().unwrap() = Some((Instant::now(), url.clone()));
    url
}

/// Whether the service answers at `base`.
fn alive(base: &str) -> bool {
    ureq::get(&format!("{}/launcher/latest.json", base.trim_end_matches('/'))).timeout(Duration::from_secs(4)).call().is_ok()
}

/// A request to the service failed to connect: the next one looks for an address that answers.
pub fn failed() {
    *CACHE.lock().unwrap() = None;
}

/// `{"auth": "https://...", "mirrors": ["https://..."]}` from the pack's `endpoint` (GitHub's contents API, raw).
fn published() -> Option<(String, Vec<String>)> {
    let endpoint = config::pack().endpoint;
    if endpoint.is_empty() {
        return None;
    }
    let reply: Value = ureq::get(&endpoint)
        .timeout(Duration::from_secs(5))
        .set("Accept", "application/vnd.github.raw+json")
        .call()
        .ok()?
        .into_json()
        .ok()?;
    let https = |v: &Value| v.as_str().filter(|u| u.starts_with("https://")).map(str::to_owned);
    let mirrors = reply["mirrors"].as_array().map(|a| a.iter().filter_map(https).collect()).unwrap_or_default();
    Some((https(&reply["auth"])?, mirrors))
}

/// The names this computer is logged in as.
pub fn logged_in() -> Vec<String> {
    tokens().into_keys().collect()
}

/// A hash of this computer, which the service counts accounts by (three per computer): the system's machine
/// id, so deleting the launcher's folder doesn't make it a new one; a random id kept in the launcher's folder
/// where the system gives none.
pub fn device() -> String {
    use sha2::{Digest, Sha256};
    let id = machine_id().unwrap_or_else(|| {
        let file = config::data_dir().join("device-id.txt");
        fs::read_to_string(&file).ok().map(|s| s.trim().to_owned()).filter(|s| s.len() >= 16).unwrap_or_else(|| {
            let id: String = (0..4).map(|_| format!("{:016x}", rand_u64())).collect();
            let _ = fs::write(&file, &id);
            id
        })
    });
    Sha256::digest(format!("wcraft-device-v1:{id}").as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new().build_hasher().finish()
}

#[cfg(not(windows))]
fn machine_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|p| fs::read_to_string(p).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(windows)]
fn machine_id() -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.contains("MachineGuid"))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_owned)
}

/// POSTs to the accounts service; the service's own message on a refusal.
fn post(path: &str, body: Value, token: Option<&str>) -> Result<Value, (u16, String)> {
    let mut request = ureq::post(&url(path)).timeout(std::time::Duration::from_secs(15));
    if let Some(token) = token {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    match request.send_json(body) {
        Ok(response) => response.into_json().map_err(|e| (0, e.to_string())),
        Err(ureq::Error::Status(code, response)) => {
            let message = response
                .into_json::<Value>()
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("Сервер аккаунтов ответил {code}"));
            Err((code, message))
        }
        Err(_) => Err((0, "Сервер аккаунтов недоступен, попробуйте позже".into())),
    }
}

/// Registers (`register = true`) or logs in, keeps the token and returns the name as registered.
pub fn authenticate(register: bool, name: &str, password: &str) -> Result<String, String> {
    let path = if register { "/api/register" } else { "/api/login" };
    let body = json!({ "name": name.trim(), "password": password, "device": device() });
    let reply = post(path, body, None).map_err(|(_, m)| m)?;
    let (Some(name), Some(token)) = (reply["name"].as_str(), reply["token"].as_str()) else {
        return Err("Сервер аккаунтов ответил непонятно".into());
    };
    let mut all = tokens();
    all.insert(name.to_owned(), token.to_owned());
    save(&all)?;
    Ok(name.to_owned())
}

/// Tells the service this account is about to join from here. The service refuses launchers older than its
/// newest release, in its own words. Returns the game's session (`{"name","uuid","token"}`) for a server in
/// online mode, if the service gave one.
pub fn join(name: &str) -> Result<Option<Value>, String> {
    let mut all = tokens();
    let Some(token) = all.get(name).cloned() else {
        return Err(format!("Войдите в аккаунт {name}: меню ника → «Другой аккаунт»"));
    };
    let custom_mods = config::Settings::load().experimental_mods.then(crate::mods::mine);
    let body = json!({ "version": env!("CARGO_PKG_VERSION"), "custom_mods": custom_mods });
    match post("/api/join", body, Some(&token)) {
        Ok(reply) => Ok(Some(reply["session"].clone()).filter(|s| s["token"].is_string())),
        Err((401, _)) => {
            all.remove(name);
            save(&all)?;
            Err(format!("Вход в аккаунт {name} устарел: войдите снова"))
        }
        Err((_, message)) => Err(message),
    }
}

/// Forgets the account's token here and ends its session on the service, if it can be reached.
pub fn logout(name: &str) -> Result<(), String> {
    let mut all = tokens();
    if let Some(token) = all.remove(name) {
        let _ = post("/api/logout", json!({}), Some(&token));
        save(&all)?;
    }
    Ok(())
}

/// The token `name` is logged in with here, or why there is none.
fn logged(name: &str) -> Result<String, String> {
    token(name).ok_or_else(|| format!("Войдите в аккаунт {name}: меню ника → «Другой аккаунт»"))
}

/// A refusal of 401 on a change means the login is stale (a wrong password says so in words).
fn changed(reply: Result<Value, (u16, String)>) -> Result<Value, String> {
    reply.map_err(|(_, m)| m)
}

/// When the account was made and when its name last changed (Unix seconds).
pub fn me(name: &str) -> Result<(u64, Option<u64>), String> {
    let token = logged(name)?;
    let reply: Value = ureq::get(&url("/api/me"))
        .set("Authorization", &format!("Bearer {token}"))
        .timeout(Duration::from_secs(15))
        .call()
        .map_err(|_| "Сервер аккаунтов недоступен, попробуйте позже")?
        .into_json()
        .map_err(|e| e.to_string())?;
    Ok((reply["created"].as_u64().unwrap_or(0), reply["renamed"].as_u64()))
}

/// A new password; the account's other logins on other computers end.
pub fn change_password(name: &str, current: &str, new: &str) -> Result<(), String> {
    changed(post("/api/password", json!({ "password": current, "new": new }), Some(&logged(name)?))).map(drop)
}

/// A new name for the account: the service moves the player's data on the server along with it. The token
/// is kept under the new name. Returns the name as registered.
pub fn rename(name: &str, password: &str, new: &str) -> Result<String, String> {
    let token = logged(name)?;
    let reply = changed(post("/api/rename", json!({ "password": password, "name": new.trim() }), Some(&token)))?;
    let new = reply["name"].as_str().ok_or("Сервер аккаунтов ответил непонятно")?.to_owned();
    let mut all = tokens();
    all.remove(name);
    all.insert(new.clone(), token);
    save(&all)?;
    Ok(new)
}

/// Deletes the account on the service and forgets its token here.
pub fn delete(name: &str, password: &str) -> Result<(), String> {
    changed(post("/api/delete", json!({ "password": password }), Some(&logged(name)?)))?;
    let mut all = tokens();
    all.remove(name);
    save(&all)
}

/// Ends the account's logins on other computers.
pub fn logout_others(name: &str) -> Result<u64, String> {
    Ok(changed(post("/api/logout_others", json!({}), Some(&logged(name)?)))?["ended"].as_u64().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    /// The published address is the live service's: `cargo test -- --ignored endpoint_is_the_live_service`.
    #[test]
    #[ignore]
    fn endpoint_is_the_live_service() {
        let url = super::published().expect("endpoint.json answers").0;
        assert_eq!(url, "https://wcraft-play.duckdns.org");
        assert_eq!(ureq::get(&format!("{url}/api/skins")).call().unwrap().status(), 200);
    }
}
