//! Experimental (`Settings::experimental_mods`): the player's own native mods compiled into the game together with
//! WCraft's. WCraft's mods come as sources with the pack (`pack.json` `mods_source`, `release-pack.sh --mods-source`),
//! the player's are unpacked `.hnmod`s in `<data>/mods/user/<id>/`; `horizon-launcher native-build` compiles both on
//! the pack's Horizon with the CLI's own Rust into a `native-*` version. Play takes that version while it was built
//! from exactly the mods and pack there are now (`ready_build`), else the pack's own game program.
use crate::horizon::{self, Emit, Slot};
use crate::{config, update};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A mod file no bigger than this: the registry's own limit for an `.hnmod` is 64 MiB unpacked.
const MAX_FILE: usize = 64 << 20;
const MAX_ENTRIES: usize = 4096;

#[derive(Serialize, Clone)]
pub struct Mod {
    pub id: String,
    pub version: String,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct State {
    /// WCraft's mods, always built in.
    pub ours: Vec<Mod>,
    pub mine: Vec<Mod>,
    /// none (no own mod is on: the pack's game plays), missing, stale or ready.
    pub build: &'static str,
    /// The pack publishes WCraft's mod sources (or `WCRAFT_MODS_SRC` points at them): building is possible.
    pub sources: bool,
    /// Bytes Rust, the compile cache and the built game take.
    pub toolchain_size: u64,
}

fn root() -> PathBuf {
    config::data_dir().join("mods")
}

fn user_dir() -> PathBuf {
    root().join("user")
}

fn off_file() -> PathBuf {
    root().join("off.json")
}

fn record_file() -> PathBuf {
    root().join("build.json")
}

fn off() -> Vec<String> {
    fs::read(off_file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// A mod folder's id and version: an unpacked `.hnmod` (`horizon-native.json`) or a bundle (`horizon-mixin.toml`).
fn manifest(dir: &Path) -> Option<(String, String)> {
    if let Ok(bytes) = fs::read(dir.join("horizon-native.json")) {
        let v: Value = serde_json::from_slice(&bytes).ok()?;
        return Some((v["id"].as_str()?.to_string(), v["version"].as_str().unwrap_or("").to_string()));
    }
    let text = fs::read_to_string(dir.join("horizon-mixin.toml")).ok()?;
    let (mut id, mut version, mut in_mod) = (None, String::new(), false);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_mod = line == "[mod]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_mod) else { continue };
        let value = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "id" => id = Some(value),
            "version" => version = value,
            _ => {}
        }
    }
    Some((id?, version))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// WCraft's mods as the pack lists them (the ones compiled into its game program).
fn ours() -> Vec<Mod> {
    let pack = config::pack();
    let listed = pack.client.map(|c| c.mods).unwrap_or_default();
    let mut mods: Vec<Mod> = listed
        .iter()
        .filter_map(|m| Some(Mod { id: m["id"].as_str()?.into(), version: m["version"].as_str().unwrap_or("").into(), enabled: true }))
        .collect();
    if mods.is_empty() {
        if let Some(dir) = dev_sources() {
            mods = mod_dirs(&dir).into_iter().map(|(_, id, version)| Mod { id, version, enabled: true }).collect();
        }
    }
    mods.sort_by(|a, b| a.id.cmp(&b.id));
    mods
}

/// The mod folders right under `dir`, by name, with their id and version.
fn mod_dirs(dir: &Path) -> Vec<(PathBuf, String, String)> {
    let mut found: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| manifest(&e.path()).map(|(id, version)| (e.path(), id, version)))
        .collect();
    found.sort_by(|a, b| a.1.cmp(&b.1));
    found
}

pub fn mine() -> Vec<Mod> {
    let off = off();
    mod_dirs(&user_dir())
        .into_iter()
        .map(|(_, id, version)| Mod { enabled: !off.contains(&id), id, version })
        .collect()
}

/// A folder of WCraft's mod sources given for development, in place of the pack's.
fn dev_sources() -> Option<PathBuf> {
    std::env::var_os("WCRAFT_MODS_SRC").map(PathBuf::from).filter(|p| p.is_dir())
}

/// Hashes every file under `dir` with its path, in a fixed order.
fn hash_tree(hash: &mut Sha256, dir: &Path, rel: &str) {
    let mut entries: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = format!("{rel}/{}", e.file_name().to_string_lossy());
        let path = e.path();
        if path.is_dir() {
            hash_tree(hash, &path, &name);
        } else if let Ok(bytes) = fs::read(&path) {
            hash.update(name.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
    }
}

/// What a build is made of: the pack's Horizon, WCraft's mod sources and the player's mods that are on. None when no
/// own mod is on or WCraft's sources are unknown: then there is nothing to build.
fn current_key() -> Option<String> {
    let mine: Vec<Mod> = mine().into_iter().filter(|m| m.enabled).collect();
    if mine.is_empty() {
        return None;
    }
    let pack = config::pack();
    let mut hash = Sha256::new();
    hash.update(pack.horizon_version.as_bytes());
    match (dev_sources(), &pack.mods_source) {
        (Some(dir), _) => hash_tree(&mut hash, &dir, "wcraft"),
        (None, Some(src)) => hash.update(src.sha256.as_bytes()),
        (None, None) => return None,
    }
    for m in &mine {
        hash_tree(&mut hash, &user_dir().join(&m.id), &format!("user/{}", m.id));
    }
    Some(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn recorded() -> Option<(String, String)> {
    let v: Value = serde_json::from_slice(&fs::read(record_file()).ok()?).ok()?;
    Some((v["key"].as_str()?.to_string(), v["tag"].as_str()?.to_string()))
}

/// The version the last build made, while the experimental mode is on: setup keeps it installed.
pub fn kept_build() -> Option<String> {
    config::Settings::load().experimental_mods.then(recorded).flatten().map(|(_, tag)| tag)
}

/// The built version to play: made of exactly the mods and pack there are now. The caller checks it is installed.
pub fn ready_build() -> Option<String> {
    if !config::Settings::load().experimental_mods {
        return None;
    }
    let (key, tag) = recorded()?;
    (current_key()? == key).then_some(tag)
}

pub fn state(installed: impl Fn(&str) -> bool) -> State {
    let sources = dev_sources().is_some() || config::pack().mods_source.is_some();
    let build = match (current_key(), recorded()) {
        (None, _) => "none",
        (Some(_), None) => "missing",
        (Some(key), Some((built, tag))) if key == built && installed(&tag) => "ready",
        (Some(_), Some((_, tag))) if installed(&tag) => "stale",
        _ => "missing",
    };
    State { ours: ours(), mine: mine(), build, sources, toolchain_size: toolchain_size() }
}

/// Unpacks `name`'s bytes (an `.hnmod`, a native `.hmod` or a `.zip` of a mod folder) into the player's mods, in place
/// of an older version of the same mod. Returns its id.
pub fn add(name: &str, bytes: &[u8]) -> Result<String, String> {
    let lower = name.to_lowercase();
    if ![".hnmod", ".hmod", ".zip"].iter().any(|ext| lower.ends_with(ext)) {
        return Err("Нужен файл мода: .hnmod, .hmod или .zip".into());
    }
    if bytes.len() > MAX_FILE {
        return Err("Файл больше 64 МБ: это не мод".into());
    }
    let tmp = root().join("incoming");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let result = unzip(bytes, &tmp).and_then(|()| take_mod(&tmp));
    let _ = fs::remove_dir_all(&tmp);
    result
}

fn unzip(bytes: &[u8], to: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "Файл не открылся как архив мода".to_string())?;
    if zip.len() > MAX_ENTRIES {
        return Err("В архиве слишком много файлов".into());
    }
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { return Err("В архиве недопустимый путь".into()) };
        let path = to.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        total += entry.size();
        if total > MAX_FILE as u64 * 4 {
            return Err("Архив распаковывается в слишком большой объём".into());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = Vec::new();
        entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
        fs::write(&path, out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The shallowest folder under `dir` that is a mod (a `.zip` of a repository has the mod one or two folders down).
fn find_mod(dir: &Path, depth: u8) -> Option<PathBuf> {
    if manifest(dir).is_some() {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    let mut subs: Vec<_> = fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    subs.sort();
    subs.iter().find_map(|p| manifest(p).is_some().then(|| p.clone())).or_else(|| subs.iter().find_map(|p| find_mod(p, depth - 1)))
}

fn take_mod(tmp: &Path) -> Result<String, String> {
    let Some(dir) = find_mod(tmp, 3) else {
        let content = tmp.join("horizon-mod.toml").is_file();
        return Err(if content {
            "Это мод без нативного кода (только ресурсы и данные): такие лаунчер пока не ставит".into()
        } else {
            "В архиве нет мода: нужен horizon-native.json или horizon-mixin.toml".into()
        });
    };
    let (id, _) = manifest(&dir).ok_or("Мод без id")?;
    if !valid_id(&id) {
        return Err(format!("Странный id мода: «{id}»"));
    }
    if ours().iter().any(|m| m.id == id) {
        return Err(format!("Мод «{id}» уже есть в сборке WCraft"));
    }
    let target = user_dir().join(&id);
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(user_dir()).map_err(|e| e.to_string())?;
    fs::rename(&dir, &target).map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn set(id: &str, on: bool) -> Result<(), String> {
    let mut off: Vec<String> = off().into_iter().filter(|o| o != id).collect();
    if !on {
        off.push(id.to_string());
    }
    fs::create_dir_all(root()).map_err(|e| e.to_string())?;
    fs::write(off_file(), json!(off).to_string()).map_err(|e| e.to_string())
}

pub fn remove(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("Нет такого мода".into());
    }
    match fs::remove_dir_all(user_dir().join(id)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
        _ => {}
    }
    set(id, true)
}

pub fn folder() -> Result<PathBuf, String> {
    fs::create_dir_all(user_dir()).map_err(|e| e.to_string())?;
    Ok(user_dir())
}

/// WCraft's mod sources: `WCRAFT_MODS_SRC`, else the pack's archive, downloaded and unpacked once per version.
fn our_sources(emit: &Emit) -> Result<PathBuf, String> {
    if let Some(dir) = dev_sources() {
        return Ok(dir);
    }
    let src = config::pack().mods_source.ok_or("Сервер пока не публикует исходники модов WCraft: сборка со своими модами недоступна")?;
    let dir = root().join("wcraft").join(src.sha256.get(..12).unwrap_or(&src.sha256));
    if dir.join(".done").is_file() {
        return Ok(dir);
    }
    emit("step", Value::String("Скачиваю исходники модов WCraft".into()));
    let file = root().join("wcraft-src.zip");
    fs::create_dir_all(root()).map_err(|e| e.to_string())?;
    let progress = |done: u64, total: u64| emit("progress", json!({ "done": done, "total": total }));
    let downloaded = update::download_client(&src, &file, &progress);
    emit("progress", Value::Null);
    downloaded?;
    let bytes = fs::read(&file).map_err(|e| e.to_string());
    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir_all(root().join("wcraft"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    unzip(&bytes?, &dir)?;
    fs::write(dir.join(".done"), "").map_err(|e| e.to_string())?;
    Ok(dir)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)?.flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type()?.is_dir() {
            copy_dir(&src, &dst)?;
        } else if e.file_type()?.is_file() {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Compiles the game with WCraft's mods and the player's that are on; the instance then plays it.
pub fn build(slot: &Slot, emit: &Emit) -> Result<(), String> {
    let step = |text: &str| emit("step", Value::String(text.to_string()));
    let mine: Vec<Mod> = mine().into_iter().filter(|m| m.enabled).collect();
    if mine.is_empty() {
        return Err("Включи хотя бы один свой мод".into());
    }
    // The pack's version installed and prepared: the build copies its game files.
    horizon::setup(slot, emit)?;
    let pack = config::pack();

    step("Проверяю, может ли компьютер собрать игру");
    let rust = horizon::json(&["rust", "status"])?;
    if let Some(problem) = rust["linker"].as_str() {
        return Err(format!("Не хватает программ для сборки. {problem}"));
    }
    if rust["installed"].is_null() {
        step("Скачиваю Rust для сборки (один раз, несколько сотен МБ)");
        horizon::run(&["rust", "install"], slot, emit)?;
    }
    horizon::check_cancel()?;

    let ours = our_sources(emit)?;
    let key = current_key().ok_or("Сборка не готова: нет исходников модов WCraft")?;
    step("Собираю моды в одну папку");
    let stage = root().join("build");
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    let mut put = |dir: &Path, id: String| -> Result<(), String> {
        if ids.contains(&id) {
            return Err(format!("Мод «{id}» добавлен дважды"));
        }
        copy_dir(dir, &stage.join(&id)).map_err(|e| format!("{}: {e}", dir.display()))?;
        ids.push(id);
        Ok(())
    };
    for (dir, id, _) in mod_dirs(&ours) {
        put(&dir, id)?;
    }
    for m in &mine {
        put(&user_dir().join(&m.id), m.id.clone())?;
    }

    step("Компилирую игру с модами: первый раз 10–30 минут, дальше быстрее");
    let stage_arg = stage.to_str().ok_or("Некорректный путь к модам")?;
    let last = horizon::run_last(
        &[
            "native-build",
            stage_arg,
            "--instance",
            &pack.instance,
            "--version",
            &pack.horizon_version,
            "--trust-native-code",
            "--allow-version-mismatch",
        ],
        slot,
        emit,
    )?;
    let tag = last.split_whitespace().next().filter(|t| t.starts_with("native-")).ok_or("Сборка не сказала, какую версию сделала")?.to_string();
    if let Some((_, old)) = recorded().filter(|(_, old)| *old != tag) {
        step("Удаляю прошлую сборку");
        if let Err(e) = horizon::json(&["remove", &old]) {
            emit("log", Value::String(format!("Сборка {old} не удалена: {e}")));
        }
    }
    fs::write(record_file(), json!({ "key": key, "tag": tag }).to_string()).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&stage);
    emit("log", Value::String(format!("Готово: {tag}")));
    Ok(())
}

/// Rust, its Cargo home, the compile cache and the built game: what `clean` frees.
fn toolchain_dirs() -> Vec<PathBuf> {
    let home = horizon::launcher_home();
    vec![home.join("rust"), home.join("cargo"), home.join("native")]
}

fn toolchain_size() -> u64 {
    let built = recorded().map(|(_, tag)| horizon::dir_size(&horizon::launcher_home().join("versions").join(tag))).unwrap_or(0);
    toolchain_dirs().iter().map(|d| horizon::dir_size(d)).sum::<u64>() + built
}

/// Removes the built game, the compile cache and Rust; the player's mods stay. The next build starts from scratch.
pub fn clean() -> Result<(), String> {
    if let Some((_, tag)) = recorded() {
        let _ = horizon::json(&["remove", &tag]);
    }
    let _ = fs::remove_file(record_file());
    // The CLI knows where its toolchain really is (an ASCII folder of its own when the data path is not).
    let _ = horizon::json(&["native-cache", "--clear"]);
    let _ = horizon::json(&["rust", "remove"]);
    for dir in toolchain_dirs() {
        match fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("{}: {e}", dir.display())),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_manifests() {
        let dir = std::env::temp_dir().join(format!("wcraft-mods-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::write(dir.join("a/horizon-native.json"), r#"{"id":"zoom","version":"2.0.0"}"#).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        fs::write(dir.join("b/horizon-mixin.toml"), "[mod]\nid = \"demo\"\nversion = \"0.1.0\"\n\n[[injection]]\nid = \"x\"\n").unwrap();
        assert_eq!(manifest(&dir.join("a")), Some(("zoom".into(), "2.0.0".into())));
        assert_eq!(manifest(&dir.join("b")), Some(("demo".into(), "0.1.0".into())));
        assert_eq!(find_mod(&dir, 2), Some(dir.join("a")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn adds_a_zipped_mod_folder() {
        use std::io::Write;
        let _guard = crate::tests::TEST_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("wcraft-mods-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        std::env::set_var("WCRAFT_HOME", &home);
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("repo-main/my_mod/horizon-native.json", opts).unwrap();
            zip.write_all(br#"{"id":"my_mod","version":"1.2.0"}"#).unwrap();
            zip.start_file("repo-main/my_mod/mixins/my_mod/horizon-mixin.toml", opts).unwrap();
            zip.write_all(b"[mod]\nid = \"my_mod\"\n").unwrap();
            zip.finish().unwrap();
        }
        assert_eq!(add("my_mod.zip", &bytes).unwrap(), "my_mod");
        assert!(add("x.txt", &bytes).is_err());
        let listed = mine();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].id.as_str(), listed[0].version.as_str(), listed[0].enabled), ("my_mod", "1.2.0", true));
        set("my_mod", false).unwrap();
        assert!(!mine()[0].enabled);
        assert!(current_key().is_none(), "nothing on: nothing to build");
        remove("my_mod").unwrap();
        assert!(mine().is_empty());
        let _ = fs::remove_dir_all(&home);
        std::env::remove_var("WCRAFT_HOME");
    }

    /// The whole build on a real data folder: `WCRAFT_HOME=<a copy> WCRAFT_MODS_SRC=<WCraft>/mods/client
    /// WCRAFT_E2E_MOD=<a mod .zip> cargo test e2e_build -- --ignored --nocapture` (8–30 minutes the first time).
    #[test]
    #[ignore]
    fn e2e_build() {
        let file = PathBuf::from(std::env::var("WCRAFT_E2E_MOD").expect("WCRAFT_E2E_MOD"));
        let mut settings = config::Settings::load();
        settings.experimental_mods = true;
        settings.save().unwrap();
        let id = add(&file.file_name().unwrap().to_string_lossy(), &fs::read(&file).unwrap()).unwrap();
        set(&id, true).unwrap();
        assert_eq!(state(horizon::is_installed).build, "missing");
        let emit: Emit = std::sync::Arc::new(|event, payload| println!("{event}: {payload}"));
        build(&Slot::default(), &emit).unwrap();
        let tag = ready_build().expect("a ready build");
        assert!(horizon::is_installed(&tag));
        assert_eq!(state(horizon::is_installed).build, "ready");
        // Setup now picks the build for the instance, and turning the mod off goes back to the pack's game.
        horizon::setup(&Slot::default(), &emit).unwrap();
        set(&id, false).unwrap();
        assert!(ready_build().is_none());
        assert_eq!(state(horizon::is_installed).build, "none");
        println!("built {tag}");
    }

    #[test]
    fn ids_are_plain() {
        assert!(valid_id("my_mod-2"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
    }
}
