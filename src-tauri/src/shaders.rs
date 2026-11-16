//! Shader packs of the WCraft instance, as the Horizon launcher's Shader packs tab has them: the
//! catalog is Horizon's registry (its `index.json`: names, pictures, screenshots), installing and
//! removing go through `horizon-launcher mods` (checksums checked), turning one on through
//! `horizon-launcher packs` (`pickedShaderPack` in the game's `horizon-graphics.txt`, one at most).
//! Packs added from a file are listed too.
use crate::{config, horizon};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const INDEX_URL: &str = "https://horizon-rust.github.io/horizon-mods/index.json";
/// How long a fetched index is shown before it is asked again.
const INDEX_FRESH: Duration = Duration::from_secs(600);

static INDEX: Mutex<Option<(Instant, Value)>> = Mutex::new(None);

#[derive(Serialize, Default)]
pub struct Shader {
    /// The registry's id; for a pack added from a file, its file name.
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub authors: Vec<String>,
    pub icon: Option<String>,
    pub screenshots: Vec<String>,
    pub categories: Vec<String>,
    pub downloads: u64,
    /// The registry's newest version and its file size.
    pub version: String,
    pub size: u64,
    pub page: String,
    pub license: String,
    /// Added from a file, not from the registry.
    pub local: bool,
    /// The Modrinth project slug, when the file came from Modrinth.
    pub modrinth: Option<String>,
    /// Its file in `shaderpacks/` when installed.
    pub file: Option<String>,
    pub installed_version: Option<String>,
    pub enabled: bool,
}

/// The registry's index, fetched at most every ten minutes; offline, the one the CLI keeps.
fn index() -> Value {
    let mut kept = INDEX.lock().unwrap();
    if let Some((at, index)) = kept.as_ref() {
        if at.elapsed() < INDEX_FRESH {
            return index.clone();
        }
    }
    let fetched = ureq::get(INDEX_URL)
        .set("User-Agent", "wcraft-launcher")
        .timeout(Duration::from_secs(20))
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok());
    match fetched {
        Some(index) => {
            *kept = Some((Instant::now(), index.clone()));
            index
        }
        None => kept.as_ref().map(|(_, i)| i.clone()).unwrap_or_else(|| {
            let cache = config::data_dir().join("game/mod-browser/horizon-index.json");
            std::fs::read(cache).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null)
        }),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

pub fn instance() -> Result<(String, PathBuf), String> {
    let name = config::pack().instance;
    let home = horizon::instance_home(&name).ok_or("Сначала установите игру: нажмите «Играть»")?;
    Ok((name, home))
}

/// The registry's shader packs, the installed ones marked, then the packs added from a file.
pub fn list() -> Result<Vec<Shader>, String> {
    let (inst, _) = instance()?;
    let packs = horizon::json(&["packs", "list", "--kind", "shader", "--instance", &inst])?;
    let packs = packs.as_array().cloned().unwrap_or_default();
    let enabled = |file: &str| packs.iter().any(|p| p["file"] == file && p["enabled"] == true);
    // What the browser installed: project → its file and version.
    let installed: HashMap<String, (String, String)> = horizon::json(&["mods", "list", "--instance", &inst])
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|e| e["kind"] == "shader")
        .filter_map(|e| {
            let file = e["file"].as_str()?.strip_prefix("shaderpacks/")?.to_string();
            Some((text(&e["project"]), (file, text(&e["version"]))))
        })
        .collect();

    let index = index();
    // Whole projects taken down (an entry without a checksum).
    let revoked: Vec<&str> = index["revoked"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r.get("sha256").is_none())
        .filter_map(|r| r["id"].as_str())
        .collect();
    let mut out: Vec<Shader> = index["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| matches!(p["kind"].as_str(), Some("shader" | "shaderpack")))
        .filter(|p| !revoked.contains(&p["id"].as_str().unwrap_or_default()))
        .map(|p| {
            let id = text(&p["id"]);
            let newest = &p["versions"][0];
            let (file, installed_version) = installed.get(&id).cloned().unzip();
            Shader {
                name: text(&p["name"]),
                summary: text(&p["summary"]),
                description: text(&p["description"]),
                authors: strings(&p["authors"]),
                icon: p["icon"].as_str().map(String::from),
                screenshots: strings(&p["screenshots"]),
                categories: strings(&p["categories"]),
                downloads: p["downloads"].as_u64().unwrap_or(0),
                version: text(&newest["version"]),
                size: newest["file"]["size"].as_u64().unwrap_or(0),
                page: format!("https://playhorizon.dev/mods/{id}"),
                license: text(&p["license"]),
                enabled: file.as_deref().is_some_and(enabled),
                file,
                installed_version,
                local: false,
                modrinth: None,
                id,
            }
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.downloads));

    // Installed from the registry but no longer in it, from Modrinth or from a file: shown as a file of its own.
    let from_modrinth = crate::modrinth::origins("shader");
    for p in &packs {
        let file = text(&p["file"]);
        if out.iter().any(|s| s.file.as_deref() == Some(file.as_str())) {
            continue;
        }
        out.push(Shader {
            id: file.clone(),
            name: text(&p["title"]),
            summary: text(&p["description"]),
            enabled: p["enabled"] == true,
            page: from_modrinth.get(&file).map(|slug| crate::modrinth::page("shader", slug)).unwrap_or_default(),
            modrinth: from_modrinth.get(&file).cloned(),
            file: Some(file),
            local: true,
            ..Default::default()
        });
    }
    Ok(out)
}

/// Downloads a registry pack into the instance and turns it on.
pub fn install(id: &str) -> Result<(), String> {
    let (inst, _) = instance()?;
    let entry = horizon::json(&["mods", "install", id, "--source", "horizon", "--instance", &inst])?;
    let file = entry
        .as_array()
        .and_then(|a| a.iter().find(|e| e["project"] == id))
        .and_then(|e| e["file"].as_str()?.strip_prefix("shaderpacks/").map(String::from))
        .ok_or("Шейдер скачан, но не найден в папке игры")?;
    set(Some(&file))
}

/// Puts the registry's newer version in place of the installed one; it stays on if it was.
pub fn update(id: &str) -> Result<(), String> {
    let was_on = list()?.into_iter().any(|s| s.id == id && s.enabled);
    let (inst, _) = instance()?;
    horizon::json(&["mods", "update", id, "--instance", &inst])?;
    if was_on {
        let file = list()?.into_iter().find(|s| s.id == id).and_then(|s| s.file);
        set(file.as_deref())?;
    }
    Ok(())
}

/// Turns this pack on (the one that was on goes off), or with `None` plays without shaders.
pub fn set(file: Option<&str>) -> Result<(), String> {
    let (inst, _) = instance()?;
    match file {
        Some(file) => horizon::json(&["packs", "on", file, "--kind", "shader", "--instance", &inst])?,
        None => {
            let packs = horizon::json(&["packs", "list", "--kind", "shader", "--instance", &inst])?;
            for p in packs.as_array().into_iter().flatten().filter(|p| p["enabled"] == true) {
                horizon::json(&["packs", "off", &text(&p["file"]), "--kind", "shader", "--instance", &inst])?;
            }
            Value::Null
        }
    };
    Ok(())
}

/// Turns it off and removes it: a registry pack through the browser, a file to the trash.
pub fn remove(id: &str) -> Result<(), String> {
    let (inst, _) = instance()?;
    let shader = list()?.into_iter().find(|s| s.id == id).ok_or("Такого шейдера нет")?;
    let file = shader.file.ok_or("Шейдер не установлен")?;
    if shader.enabled {
        set(None)?;
    }
    match shader.local {
        true => horizon::json(&["packs", "remove", &file, "--kind", "shader", "--instance", &inst])?,
        false => horizon::json(&["mods", "remove", id, "--instance", &inst])?,
    };
    Ok(())
}

/// Copies a shader pack file (`.zip` in the `shaders/` format, or `.hshader`) into the instance.
pub fn add(name: &str, bytes: &[u8]) -> Result<(), String> {
    add_pack("shader", name, bytes).map(|_| ())
}

/// Copies a pack file into the instance as `kind` ("shader" or "resource") and gives the file's
/// name there. A pack of the other kind is taken back out.
pub fn add_pack(kind: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
    let name = std::path::Path::new(name).file_name().and_then(|n| n.to_str()).ok_or("Нет имени файла")?;
    let lower = name.to_lowercase();
    let shader = kind == "shader";
    if !lower.ends_with(".zip") && !(shader && lower.ends_with(".hshader")) {
        return Err(if shader { "Шейдеры — это файлы .zip или .hshader" } else { "Ресурспаки — это файлы .zip" }.into());
    }
    let (inst, _) = instance()?;
    let dir = config::data_dir().join("incoming");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let added = horizon::json(&["packs", "add", &path.to_string_lossy(), "--instance", &inst]);
    let _ = std::fs::remove_file(&path);
    let added = added?;
    if added["kind"] != kind {
        let _ = horizon::json(&["packs", "remove", &text(&added["file"]), "--kind", &text(&added["kind"]), "--instance", &inst]);
        return Err(if shader { "Это пакет ресурсов, а не шейдер" } else { "Это шейдер, а не пакет ресурсов" }.into());
    }
    Ok(text(&added["file"]))
}

/// The instance's `shaderpacks/` folder, made if missing.
pub fn folder() -> Result<PathBuf, String> {
    let dir = instance()?.1.join("shaderpacks");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live registry still has shader packs in the shape the page reads.
    #[test]
    #[ignore]
    fn registry_lists_shaders() {
        let index = index();
        let shaders: Vec<_> =
            index["projects"].as_array().unwrap().iter().filter(|p| p["kind"] == "shader").collect();
        assert!(!shaders.is_empty());
        for p in shaders {
            assert!(p["name"].is_string() && p["versions"][0]["version"].is_string(), "{}", p["id"]);
        }
    }

    /// Install, switch, turn off and remove, in a data folder that has a WCraft instance
    /// (`horizon-launcher instance create WCraft` with that folder's HORIZON_* paths):
    /// `WCRAFT_HOME=<dir> HORIZON_LAUNCHER_TRASH=<dir>/trash cargo test -- --ignored shader_round_trip`
    #[test]
    #[ignore]
    fn shader_round_trip() {
        let on = || list().unwrap().into_iter().filter(|s| s.enabled).map(|s| s.id).collect::<Vec<_>>();
        let catalog = list().unwrap();
        assert!(catalog.iter().all(|s| s.file.is_none()), "starts with nothing installed");
        install("daybreak").unwrap();
        assert_eq!(on(), ["daybreak"]);
        install("volexstorm").unwrap();
        assert_eq!(on(), ["volexstorm"], "one pack on at a time");
        set(None).unwrap();
        assert!(on().is_empty());
        let daybreak = list().unwrap().into_iter().find(|s| s.id == "daybreak").unwrap();
        set(daybreak.file.as_deref()).unwrap();
        assert_eq!(on(), ["daybreak"]);
        remove("daybreak").unwrap();
        remove("volexstorm").unwrap();
        assert!(list().unwrap().iter().all(|s| s.file.is_none() && !s.enabled));
        assert!(add("x.txt", b"no").is_err());
    }
}
