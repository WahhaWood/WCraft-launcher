//! Shader packs from Modrinth: search its public API, download a project's newest `shaders/`-format
//! pack (Iris/OptiFine, which Horizon's own loader reads), check the file's SHA-512 and hand it to
//! the same path as a file the player picked. Which file came from which project is kept in
//! `modrinth-shaders.json`, so the catalog can mark what is installed.
use crate::{config, resources, shaders};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

const API: &str = "https://api.modrinth.com/v2";
/// A pack bigger than this is not a shader pack worth holding the launcher for.
const MAX_BYTES: u64 = 300 * 1024 * 1024;
const PAGE: usize = 20;

fn get(url: &str) -> Result<ureq::Response, String> {
    ureq::get(url)
        .set("User-Agent", "wcraft-launcher")
        .timeout(Duration::from_secs(30))
        .call()
        .map_err(|e| format!("Modrinth не ответил: {e}"))
}

/// `kind` is "shader" or "resource"; Modrinth calls the second "resourcepack".
fn project_type(kind: &str) -> &'static str {
    if kind == "shader" { "shader" } else { "resourcepack" }
}

fn record_path(kind: &str) -> std::path::PathBuf {
    config::data_dir().join(if kind == "shader" { "modrinth-shaders.json" } else { "modrinth-resources.json" })
}

/// Project slug → the file in the instance's pack folder it was installed as.
fn records(kind: &str) -> HashMap<String, String> {
    std::fs::read(record_path(kind)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// File in the instance's pack folder → the Modrinth project it was installed from.
pub fn origins(kind: &str) -> HashMap<String, String> {
    records(kind).into_iter().map(|(slug, file)| (file, slug)).collect()
}

pub fn page(kind: &str, slug: &str) -> String {
    format!("https://modrinth.com/{}/{slug}", project_type(kind))
}

/// The pack files the instance has of `kind`.
fn present(kind: &str) -> Vec<String> {
    let list = if kind == "shader" { shaders::list() } else { resources::list() };
    list.map(|l| l.into_iter().filter_map(|s| s.file).collect()).unwrap_or_default()
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// One page of Modrinth's projects of `kind` (most downloaded first when `query` is empty).
pub fn search(kind: &str, query: &str, offset: usize) -> Result<Vec<shaders::Shader>, String> {
    let facets = format!(r#"[["project_type:{}"]]"#, project_type(kind));
    let mut url = format!("{API}/search?facets={}&limit={PAGE}&offset={offset}&index=", enc(&facets));
    url.push_str(if query.trim().is_empty() { "downloads" } else { "relevance" });
    if !query.trim().is_empty() {
        url.push_str("&query=");
        url.push_str(&enc(query.trim()));
    }
    let found: Value = get(&url)?.into_json().map_err(|e| e.to_string())?;
    let kept = records(kind);
    let present = present(kind);
    Ok(found["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let slug = text(&p["slug"]);
            let file = kept.get(&slug).filter(|f| present.contains(f)).cloned();
            shaders::Shader {
                name: text(&p["title"]),
                summary: text(&p["description"]),
                description: text(&p["description"]),
                authors: vec![text(&p["author"])],
                icon: p["icon_url"].as_str().map(String::from),
                screenshots: strings(&p["gallery"]),
                categories: strings(&p["display_categories"]),
                downloads: p["downloads"].as_u64().unwrap_or(0),
                page: page(kind, &slug),
                license: text(&p["license"]),
                installed_version: file.as_ref().map(|_| String::new()),
                file,
                id: slug,
                ..Default::default()
            }
        })
        .collect())
}

/// The project's screenshots at full size: search only gives 350 px thumbnails, the project's own entry has
/// the originals (`raw_url`).
pub fn gallery(slug: &str) -> Result<Vec<String>, String> {
    if slug.is_empty() || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("Неверный проект".into());
    }
    let project: Value = get(&format!("{API}/project/{slug}"))?.into_json().map_err(|e| e.to_string())?;
    Ok(project["gallery"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| g["raw_url"].as_str().or_else(|| g["url"].as_str()).map(String::from))
        .collect())
}

/// Downloads the newest version of `slug` that has a pack file, and turns it on.
pub fn install(kind: &str, slug: &str) -> Result<(), String> {
    if slug.is_empty() || !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("Неверный проект".into());
    }
    let versions: Value = get(&format!("{API}/project/{slug}/version"))?.into_json().map_err(|e| e.to_string())?;
    // Newest first; take the first version with a .zip file.
    let file = versions
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|v| {
            let files = v["files"].as_array()?;
            let f = files
                .iter()
                .find(|f| f["primary"] == true && text(&f["filename"]).to_lowercase().ends_with(".zip"))
                .or_else(|| files.iter().find(|f| text(&f["filename"]).to_lowercase().ends_with(".zip")))?;
            Some(f.clone())
        })
        .ok_or("У проекта нет файла .zip")?;
    let name = text(&file["filename"]);
    if file["size"].as_u64().unwrap_or(0) > MAX_BYTES {
        return Err("Файл слишком большой".into());
    }
    let mut bytes = Vec::new();
    get(&text(&file["url"]))?
        .into_reader()
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Загрузка прервалась: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Файл слишком большой".into());
    }
    let want = text(&file["hashes"]["sha512"]);
    let got: String = Sha512::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if !want.eq_ignore_ascii_case(&got) {
        return Err("Контрольная сумма файла не сошлась, установка отменена".into());
    }
    let installed = shaders::add_pack(kind, &name, &bytes)?;
    let mut kept = records(kind);
    kept.insert(slug.to_string(), installed.clone());
    let _ = std::fs::write(record_path(kind), serde_json::to_vec(&kept).unwrap_or_default());
    if kind == "shader" {
        shaders::set(Some(&installed))
    } else {
        resources::set(&installed, true)
    }
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_query() {
        assert_eq!(enc("a b/ё"), "a%20b%2F%D1%91");
        assert_eq!(enc(r#"[["x:y"]]"#), "%5B%5B%22x%3Ay%22%5D%5D");
    }

    /// The live API still answers in the shape `search` reads.
    #[test]
    #[ignore]
    fn modrinth_lists_shaders() {
        let found: Value = get(&format!("{API}/search?facets={}&limit=3", enc(r#"[["project_type:shader"]]"#)))
            .unwrap()
            .into_json()
            .unwrap();
        assert!(found["hits"][0]["slug"].is_string() && found["hits"][0]["title"].is_string());
    }

    /// Search, install, turn off and remove one shader and one resource pack from the live Modrinth,
    /// in a data folder that has a WCraft instance (as `shaders::tests::shader_round_trip`):
    /// `WCRAFT_HOME=<dir> HORIZON_LAUNCHER_TRASH=<dir>/trash cargo test -- --ignored modrinth_round_trip`
    #[test]
    #[ignore]
    fn modrinth_round_trip() {
        for kind in ["shader", "resource"] {
            let found = search(kind, "", 0).unwrap();
            assert_eq!(found.len(), PAGE);
            assert!(found.iter().all(|s| s.file.is_none() && !s.id.is_empty()));
            // The smallest download of the first page keeps the run short.
            let slug = found.iter().find(|s| s.id != "complementary-reimagined").unwrap().id.clone();
            install(kind, &slug).unwrap();
            let again = search(kind, &slug, 0).unwrap();
            let file = again.iter().find(|s| s.id == slug).and_then(|s| s.file.clone()).expect("marked installed");
            if kind == "shader" {
                assert!(shaders::list().unwrap().iter().any(|s| s.enabled && s.file.as_deref() == Some(&file)));
                shaders::set(None).unwrap();
                let id = shaders::list().unwrap().into_iter().find(|s| s.file.as_deref() == Some(&file)).unwrap().id;
                shaders::remove(&id).unwrap();
            } else {
                assert!(resources::list().unwrap().iter().any(|s| s.enabled && s.file.as_deref() == Some(&file)));
                resources::set(&file, false).unwrap();
                resources::remove(&file).unwrap();
            }
            assert!(search(kind, &slug, 0).unwrap().iter().all(|s| s.file.is_none()));
        }
        assert!(install("shader", "../x").is_err());
    }
}
