//! Resource packs of the WCraft instance: the ones in `resourcepacks/`, turned on and off through
//! `horizon-launcher packs` (several can be on; one turned on goes on top), added from a file or
//! from Modrinth (`modrinth.rs`). Horizon has no catalog of its own for them.
use crate::{horizon, shaders};
use serde_json::Value;
use std::path::PathBuf;

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// The instance's resource packs, those on first.
pub fn list() -> Result<Vec<shaders::Shader>, String> {
    let (inst, _) = shaders::instance()?;
    let packs = horizon::json(&["packs", "list", "--kind", "resource", "--instance", &inst])?;
    let from_modrinth = crate::modrinth::origins("resource");
    let mut out: Vec<shaders::Shader> = packs
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let file = text(&p["file"]);
            shaders::Shader {
                id: file.clone(),
                name: if p["title"].as_str().is_some_and(|t| !t.is_empty()) { text(&p["title"]) } else { file.clone() },
                summary: text(&p["description"]),
                enabled: p["enabled"] == true,
                page: from_modrinth.get(&file).map(|slug| crate::modrinth::page("resource", slug)).unwrap_or_default(),
                modrinth: from_modrinth.get(&file).cloned(),
                file: Some(file),
                local: true,
                ..Default::default()
            }
        })
        .collect();
    out.sort_by_key(|s| !s.enabled);
    Ok(out)
}

/// Turns a pack on (on top of the others) or off.
pub fn set(file: &str, on: bool) -> Result<(), String> {
    let (inst, _) = shaders::instance()?;
    horizon::json(&["packs", if on { "on" } else { "off" }, file, "--kind", "resource", "--instance", &inst])?;
    Ok(())
}

/// Turns it off and moves the file to the trash.
pub fn remove(file: &str) -> Result<(), String> {
    let (inst, _) = shaders::instance()?;
    horizon::json(&["packs", "remove", file, "--kind", "resource", "--instance", &inst])?;
    Ok(())
}

pub fn add(name: &str, bytes: &[u8]) -> Result<(), String> {
    shaders::add_pack("resource", name, bytes).map(|_| ())
}

/// The instance's `resourcepacks/` folder, made if missing.
pub fn folder() -> Result<PathBuf, String> {
    let dir = shaders::instance()?.1.join("resourcepacks");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
