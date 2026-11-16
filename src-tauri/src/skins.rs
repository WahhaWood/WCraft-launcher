//! The skin library: PNGs the player saved, and the game's default skins bundled with the
//! launcher (`skins/`, copied from the game's resources), so they show before the first install.
//!
//! On the server: the active skin is published as the account's (`publish`), and everyone's published skins are
//! downloaded (`sync`) into the game's skin cache (`install`). The server mod (mods/server/wcraft-auth) gives each player a
//! `textures` property with `textures.minecraft.net/texture/<sha256 of the PNG>`; Horizon reads that texture from
//! its disk cache before it would download it, so the skin shows without ever touching Mojang's servers.
use crate::{auth, config, horizon};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{fs, path::PathBuf};

#[derive(Deserialize, Serialize, Clone)]
pub struct Saved {
    pub id: String,
    pub name: String,
    /// classic | slim
    pub model: String,
}

#[derive(Deserialize, Serialize, Default)]
struct Index {
    active: Option<String>,
    items: Vec<Saved>,
}

#[derive(Serialize)]
pub struct Skin {
    pub id: String,
    pub name: String,
    pub model: String,
    pub url: String,
}

#[derive(Serialize)]
pub struct Library {
    pub active: Option<String>,
    pub saved: Vec<Skin>,
    pub defaults: Vec<Skin>,
}

fn dir() -> PathBuf {
    let d = config::data_dir().join("skins");
    let _ = fs::create_dir_all(&d);
    d
}

fn index() -> Index {
    fs::read(dir().join("index.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(index: &Index) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(index).map_err(|e| e.to_string())?;
    fs::write(dir().join("index.json"), json).map_err(|e| e.to_string())
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

/// The default skins, each with the arms Mojang ships it with first.
const DEFAULTS: [(&str, &str, &[u8]); 9] = [
    ("steve", "classic", include_bytes!("../skins/steve.png")),
    ("alex", "slim", include_bytes!("../skins/alex.png")),
    ("ari", "classic", include_bytes!("../skins/ari.png")),
    ("efe", "slim", include_bytes!("../skins/efe.png")),
    ("kai", "classic", include_bytes!("../skins/kai.png")),
    ("makena", "slim", include_bytes!("../skins/makena.png")),
    ("noor", "slim", include_bytes!("../skins/noor.png")),
    ("sunny", "classic", include_bytes!("../skins/sunny.png")),
    ("zuri", "classic", include_bytes!("../skins/zuri.png")),
];

fn defaults() -> Vec<Skin> {
    DEFAULTS
        .iter()
        .map(|(name, model, png)| {
            let mut title = name.to_string();
            title[..1].make_ascii_uppercase();
            Skin { id: format!("default:{name}"), name: title, model: model.to_string(), url: data_url(png) }
        })
        .collect()
}

pub fn library() -> Library {
    let index = index();
    let saved = index
        .items
        .iter()
        .filter_map(|s| {
            let png = fs::read(dir().join(format!("{}.png", s.id))).ok()?;
            Some(Skin { id: s.id.clone(), name: s.name.clone(), model: s.model.clone(), url: data_url(&png) })
        })
        .collect();
    Library { active: index.active, saved, defaults: defaults() }
}

/// A skin is a 64x64 PNG, or the old 64x32 one.
fn check_png(png: &[u8]) -> Result<(), String> {
    const MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 24 || !png.starts_with(MAGIC) || &png[12..16] != b"IHDR" {
        return Err("Это не PNG".into());
    }
    let be = |i: usize| u32::from_be_bytes([png[i], png[i + 1], png[i + 2], png[i + 3]]);
    match (be(16), be(20)) {
        (64, 64) | (64, 32) => Ok(()),
        (w, h) => Err(format!("Скин должен быть 64×64 или 64×32, а этот {w}×{h}")),
    }
}

const CATALOG: &str = "https://api.millida.net/v2/skins";

#[derive(Serialize)]
pub struct CatalogSkin {
    pub id: String,
    pub title: String,
    pub model: String,
    pub preview: String,
    pub wearers: u64,
}

#[derive(Serialize)]
pub struct CatalogPage {
    pub items: Vec<CatalogSkin>,
    pub pages: u64,
}

/// One agent for the catalog: it keeps the connection, so a search skips the TLS handshake.
fn catalog_get(url: &str) -> Result<ureq::Response, String> {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT
        .get_or_init(|| ureq::AgentBuilder::new().user_agent("wcraft-launcher").timeout(Duration::from_secs(20)).build())
        .get(url)
        .call()
        .map_err(|e| format!("Каталог скинов не ответил: {e}"))
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One page of the millida.net skin catalog. `sort` is popular, new or random; `tag` is a catalog tag slug (the launcher offers a fixed set, `Skins.svelte` TAGS).
pub fn catalog_search(query: &str, page: u64, tag: &str, sort: &str) -> Result<CatalogPage, String> {
    let sort = if ["popular", "new", "random"].contains(&sort) { sort } else { "popular" };
    let mut url = format!("{CATALOG}?page={}&sort={sort}", page.max(1));
    if valid_slug(tag) {
        url.push_str("&tag=");
        url.push_str(tag);
    }
    let query = query.trim();
    if !query.is_empty() {
        url.push_str("&q=");
        for b in query.chars().take(60).collect::<String>().bytes() {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                url.push(b as char);
            } else {
                url.push_str(&format!("%{b:02X}"));
            }
        }
    }
    let v: Value = catalog_get(&url)?.into_json().map_err(|e| e.to_string())?;
    let items = v["items"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|i| {
                    Some(CatalogSkin {
                        id: i["id"].as_str().filter(|id| valid_catalog_id(id))?.to_string(),
                        title: i["title"].as_str().unwrap_or("Скин").to_string(),
                        model: if i["model"] == "slim" { "slim" } else { "classic" }.into(),
                        preview: i["renderUrl"].as_str().filter(|u| u.starts_with("https://api.millida.net/"))?.to_string(),
                        wearers: i["wearers"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(CatalogPage { items, pages: v["pages"].as_u64().unwrap_or(1) })
}

fn valid_catalog_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn catalog_download(id: &str) -> Result<Vec<u8>, String> {
    if !valid_catalog_id(id) {
        return Err("Неверный скин".into());
    }
    let mut png = Vec::new();
    catalog_get(&format!("{CATALOG}/{id}/download"))?.into_reader().take(1 << 20).read_to_end(&mut png).map_err(|e| e.to_string())?;
    check_png(&png)?;
    Ok(png)
}

/// A catalog skin's PNG as a data URL, to try it on without saving it.
pub fn catalog_png(id: &str) -> Result<String, String> {
    catalog_download(id).map(|png| data_url(&png))
}

/// Download a catalog skin into the library and wear it.
pub fn catalog_install(id: &str, title: &str, model: &str) -> Result<String, String> {
    add(title, if model == "slim" { "slim" } else { "classic" }, &catalog_download(id)?)
}

pub fn add(name: &str, model: &str, png: &[u8]) -> Result<String, String> {
    if png.len() > 1 << 20 {
        return Err("Файл слишком большой".into());
    }
    check_png(png)?;
    let id: String = Sha256::digest(png).iter().take(8).map(|b| format!("{b:02x}")).collect();
    fs::write(dir().join(format!("{id}.png")), png).map_err(|e| e.to_string())?;
    let mut index = index();
    if !index.items.iter().any(|s| s.id == id) {
        let name = name.trim().chars().take(40).collect::<String>();
        index.items.insert(0, Saved { id: id.clone(), name, model: model.to_string() });
    }
    index.active = Some(id.clone());
    save(&index)?;
    Ok(id)
}

pub fn remove(id: &str) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Скин не найден".into());
    }
    let mut index = index();
    index.items.retain(|s| s.id != id);
    if index.active.as_deref() == Some(id) {
        index.active = None;
    }
    save(&index)?;
    let _ = fs::remove_file(dir().join(format!("{id}.png")));
    Ok(())
}

pub fn active_skin_file() -> Option<(PathBuf, String)> {
    let index = index();
    let id = index.active?;
    if let Some(name) = id.strip_prefix("default:") {
        // The CLI takes a file: the bundled PNG is written next to the saved ones.
        let (_, model, png) = DEFAULTS.iter().find(|(n, _, _)| *n == name)?;
        let path = dir().join(format!("default-{name}.png"));
        if fs::read(&path).ok().as_deref() != Some(*png) {
            fs::write(&path, png).ok()?;
        }
        return Some((path, model.to_string()));
    }
    let skin = index.items.iter().find(|s| s.id == id)?;
    let path = dir().join(format!("{id}.png"));
    if path.is_file() {
        Some((path, skin.model.clone()))
    } else {
        None
    }
}

pub fn select(id: &str) -> Result<(), String> {
    let mut index = index();
    index.active = Some(id.to_string());
    save(&index)?;
    if let Some(acc) = horizon::status().account {
        if acc.offline {
            if let Some((path, model)) = active_skin_file() {
                let _ = horizon::account_set_skin(&acc.name, &path, &model);
            }
        }
    }
    Ok(())
}

pub fn set_model(id: &str, model: &str) -> Result<(), String> {
    let mut index = index();
    let skin = index.items.iter_mut().find(|s| s.id == id).ok_or("Скин не найден")?;
    skin.model = model.to_string();
    save(&index)?;
    if index.active.as_deref() == Some(id) {
        if let Some(acc) = horizon::status().account {
            if acc.offline {
                if let Some((path, model)) = active_skin_file() {
                    let _ = horizon::account_set_skin(&acc.name, &path, &model);
                }
            }
        }
    }
    Ok(())
}

/// A skin a player published on the server (`GET /api/skins`).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Published {
    pub name: String,
    /// The PNG's SHA-256 in hex: the last part of the texture URL the server gives the game.
    pub hash: String,
    pub model: String,
}

const TIMEOUT: Duration = Duration::from_secs(15);

/// Downloaded skins of the server's players, `<hash>.png`, and the last list (`index.json`).
fn players_dir() -> PathBuf {
    let d = dir().join("players");
    let _ = fs::create_dir_all(&d);
    d
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The accounts service's message on a refusal.
fn http_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, response) => response
            .into_json::<Value>()
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("Сервер аккаунтов ответил {code}")),
        _ => "Сервер аккаунтов недоступен".into(),
    }
}

/// Publishes the active skin as `name`'s on the server; with none chosen, the name's default skin shows.
pub fn publish(name: &str) -> Result<(), String> {
    let token = auth::token(name).ok_or_else(|| format!("{name}: не выполнен вход в аккаунт"))?;
    let bearer = format!("Bearer {token}");
    let reply = match active_skin_file() {
        Some((path, model)) => {
            let png = fs::read(&path).map_err(|e| e.to_string())?;
            ureq::put(&auth::url(&format!("/api/skin?model={model}")))
                .timeout(TIMEOUT)
                .set("Authorization", &bearer)
                .set("Content-Type", "image/png")
                .send_bytes(&png)
        }
        None => ureq::delete(&auth::url("/api/skin")).timeout(TIMEOUT).set("Authorization", &bearer).call(),
    };
    reply.map(drop).map_err(http_error)
}

/// Publishes the active skin for the account that plays, if it is logged in here; quietly, in the background.
pub fn publish_active() {
    std::thread::spawn(|| {
        let Some(acc) = horizon::status().account else { return };
        if auth::token(&acc.name).is_some() {
            if let Err(e) = publish(&acc.name) {
                eprintln!("wcraft: skin not published: {e}");
            }
        }
    });
}

/// Everyone's published skins: asks the server and downloads the PNGs not here yet.
pub fn sync() -> Result<Vec<Published>, String> {
    let reply: Value = ureq::get(&auth::url("/api/skins"))
        .timeout(TIMEOUT)
        .call()
        .map_err(http_error)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let list: Vec<Published> = serde_json::from_value(reply["skins"].clone()).map_err(|e| e.to_string())?;
    let list: Vec<Published> = list.into_iter().filter(|s| valid_hash(&s.hash)).collect();
    for skin in &list {
        let file = players_dir().join(format!("{}.png", skin.hash));
        if file.is_file() {
            continue;
        }
        let response = ureq::get(&auth::url(&format!("/api/textures/{}", skin.hash)))
            .timeout(TIMEOUT)
            .call()
            .map_err(http_error)?;
        let mut png = Vec::new();
        response.into_reader().take(1 << 20).read_to_end(&mut png).map_err(|e| e.to_string())?;
        // Kept under its hash only when it is what the hash says.
        if hex(&Sha256::digest(&png)) == skin.hash {
            fs::write(&file, &png).map_err(|e| e.to_string())?;
        }
    }
    let json = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
    fs::write(players_dir().join("index.json"), json).map_err(|e| e.to_string())?;
    Ok(list)
}

/// The list the last `sync` got.
fn cached() -> Vec<Published> {
    fs::read(players_dir().join("index.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// The file name the game caches a texture under: the SHA-1 of its hash's UTF-16 code units
/// (`SkinManager.TextureCache`; Horizon's `skins::skin_location`).
pub fn cache_name(hash: &str) -> String {
    use sha1::Sha1;
    let units: Vec<u8> = hash.encode_utf16().flat_map(u16::to_le_bytes).collect();
    hex(&Sha1::digest(&units))
}

/// Lays the downloaded skins into the game's skin cache, `assets/skins/<xx>/<name>` in its folder.
pub fn install(list: &[Published], game_dir: &Path) {
    for skin in list {
        let Ok(png) = fs::read(players_dir().join(format!("{}.png", skin.hash))) else { continue };
        let name = cache_name(&skin.hash);
        let file = game_dir.join("assets/skins").join(&name[..2]).join(&name);
        if fs::read(&file).ok().as_deref() == Some(png.as_slice()) {
            continue;
        }
        if let Some(parent) = file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(&file, &png) {
            eprintln!("wcraft: {}: {e}", file.display());
        }
    }
}

/// Gets everyone's skins into the game's cache; without the server, what was downloaded before.
pub fn sync_into(game_dir: &Path) -> Result<(), String> {
    let result = sync();
    let list = result.as_ref().cloned().unwrap_or_else(|_| cached());
    install(&list, game_dir);
    result.map(drop)
}

/// While the game runs, brings in skins published since it started, every 30 s (a texture the game
/// failed to find it looks for again a minute later). Stops when dropped.
pub struct Keeper(Arc<AtomicBool>);

impl Drop for Keeper {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub fn keep_synced(game_dir: PathBuf) -> Keeper {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        loop {
            for _ in 0..30 {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            let _ = sync_into(&game_dir);
        }
    });
    Keeper(stop)
}

/// The server's players' published skins by name, for their faces in the launcher.
pub fn players() -> BTreeMap<String, Skin> {
    let list = sync().unwrap_or_else(|_| cached());
    list.into_iter()
        .filter_map(|p| {
            let png = fs::read(players_dir().join(format!("{}.png", p.hash))).ok()?;
            let skin = Skin { id: p.hash, name: p.name.clone(), model: p.model, url: data_url(&png) };
            Some((p.name, skin))
        })
        .collect()
}
