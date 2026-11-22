mod applog;
mod auth;
mod bootstrap;
mod config;
mod horizon;
mod mods;
mod ping;
mod modrinth;
mod resources;
mod shaders;
mod skins;
mod update;

use horizon::{Emit, Slot};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Default)]
struct Tasks {
    /// Setup or sign-in: one at a time, cancellable.
    task: Arc<Slot>,
    game: Arc<Slot>,
    playing: Arc<Mutex<bool>>,
    /// The window was closed while the game runs: quit when the game exits. Until then the
    /// launcher stays, hidden, because the game's output goes through it.
    quit_after_game: Arc<AtomicBool>,
}

fn emitter(app: &AppHandle) -> Emit {
    let app = app.clone();
    Arc::new(move |event, payload| {
        if event == "signin" {
            if let Some(link) = payload["link"].as_str() {
                let _ = app.opener().open_url(link, None::<&str>);
            }
        }
        if matches!(event, "step" | "log") {
            if let Some(text) = payload.as_str() {
                applog::line(&format!("{event}: {text}"));
            }
        }
        let _ = app.emit(event, payload);
    })
}

#[derive(serde::Serialize)]
struct State_ {
    #[serde(flatten)]
    status: horizon::Status,
    playing: bool,
    version: &'static str,
    /// Names logged in to their WCraft account here.
    logged_in: Vec<String>,
}

#[tauri::command(async)]
fn status(tasks: State<Tasks>) -> State_ {
    State_ {
        status: horizon::status(),
        playing: *tasks.playing.lock().unwrap(),
        version: env!("CARGO_PKG_VERSION"),
        logged_in: auth::logged_in(),
    }
}

#[tauri::command(async)]
fn account_offline(app: AppHandle, name: String) -> Result<horizon::Account, String> {
    if horizon::cli().is_none() {
        let emit = emitter(&app);
        emit("step", "Скачиваю Horizon Launcher".into());
        bootstrap::install(&emit)?;
    }
    horizon::account_offline(&name)
}

/// Registers or logs in to a WCraft account, then plays as it on this computer.
#[tauri::command(async)]
fn auth(app: AppHandle, register: bool, name: String, password: String) -> Result<horizon::Account, String> {
    let name = auth::authenticate(register, &name, &password)?;
    let status = horizon::status();
    if let Some(acc) = status.accounts.into_iter().find(|a| a.name == name) {
        horizon::account_select(&name)?;
        return Ok(acc);
    }
    account_offline(app, name)
}

/// Changes the account's name on the service (its saved data on the server follows), then here: the account
/// of the new name is made, picked if the old one was, and the old one forgotten.
#[tauri::command(async)]
fn account_rename(app: AppHandle, name: String, password: String, new_name: String, tasks: State<'_, Tasks>) -> Result<String, String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Ник меняется, пока игра закрыта".into());
    }
    let new = auth::rename(&name, &password, &new_name)?;
    let was_picked = horizon::status().account.is_some_and(|a| a.name == name);
    account_offline(app, new.clone())?;
    if was_picked {
        horizon::account_select(&new)?;
    }
    horizon::account_remove(&name, true)?;
    Ok(new)
}

#[tauri::command(async)]
fn account_password(name: String, current: String, new: String) -> Result<(), String> {
    auth::change_password(&name, &current, &new)
}

#[tauri::command(async)]
fn account_logout_others(name: String) -> Result<u64, String> {
    auth::logout_others(&name)
}

/// Deletes the account on the service, then forgets it here.
#[tauri::command(async)]
fn account_delete(name: String, password: String, tasks: State<'_, Tasks>) -> Result<(), String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Аккаунт удаляется, пока игра закрыта".into());
    }
    auth::delete(&name, &password)?;
    horizon::account_remove(&name, true)
}

/// When the account was made and when its name last changed (Unix seconds).
#[tauri::command(async)]
fn account_info(name: String) -> Result<(u64, Option<u64>), String> {
    auth::me(&name)
}

#[tauri::command(async)]
fn account_select(name: String) -> Result<(), String> {
    horizon::account_select(&name)
}

#[tauri::command(async)]
fn account_remove(name: String, offline: bool) -> Result<(), String> {
    auth::logout(&name)?;
    horizon::account_remove(&name, offline)
}

#[tauri::command(async)]
fn sign_in(app: AppHandle, tasks: State<Tasks>) -> Result<(), String> {
    let emit = emitter(&app);
    if horizon::cli().is_none() {
        emit("step", "Скачиваю Horizon Launcher".into());
        bootstrap::install(&emit)?;
    }
    emit("step", "Жду входа в браузере".into());
    horizon::reset_cancel();
    horizon::run(&["account", "sign-in"], &tasks.task, &emit)
}

#[tauri::command(async)]
fn sign_out(uuid: String) -> Result<(), String> {
    horizon::json(&["account", "sign-out", &uuid]).map(|_| ())
}

#[tauri::command]
fn cancel(tasks: State<Tasks>) {
    horizon::cancel(&tasks.task);
}

fn window_while_playing(app: &AppHandle, playing: bool) {
    let Some(window) = app.get_webview_window("main") else { return };
    let mode = config::Settings::load().while_playing;
    match (playing, mode.as_str()) {
        (true, "minimize") => drop(window.minimize()),
        (true, "hide") => drop(window.hide()),
        (false, "minimize") => drop(window.unminimize()),
        (false, "hide") => {
            let _ = window.show();
            let _ = window.set_focus();
        }
        _ => {}
    }
}

/// Sets everything up, then starts the game. Returns once it started; `game-exit` follows.
#[tauri::command(async)]
fn play(app: AppHandle, tasks: State<Tasks>) -> Result<(), String> {
    if std::mem::replace(&mut *tasks.playing.lock().unwrap(), true) {
        return Err("Игра уже запущена".into());
    }
    let emit = emitter(&app);
    horizon::reset_cancel();
    // The game version and server may have moved since the launcher started; without an answer, the kept ones.
    match horizon::interruptible(update::refresh_pack) {
        Err(e) => {
            *tasks.playing.lock().unwrap() = false;
            applog::line(&format!("play: pack refresh interrupted: {e}"));
            return Err(e);
        }
        // Without any pack kept, the built-in one has no game build of ours: don't start the wrong game.
        Ok(Err(e)) if config::published().is_none() => {
            *tasks.playing.lock().unwrap() = false;
            applog::line(&format!("play: no pack kept and refresh failed: {e}"));
            return Err(format!("Не удалось получить сборку WCraft, проверьте интернет и попробуйте снова: {e}"));
        }
        Ok(Err(e)) => emit("log", format!("Манифест сборки не обновлён: {e}").into()),
        Ok(Ok(changed)) => applog::line(&format!("play: pack {}", if changed { "updated" } else { "unchanged" })),
    }
    applog::line(&format!("play: {}", applog::pack_summary()));
    if let Err(e) = horizon::setup(&tasks.task, &emit) {
        *tasks.playing.lock().unwrap() = false;
        applog::line(&format!("play: setup failed: {e}"));
        return Err(e);
    }
    emit("step", "Запускаю игру".into());
    let (game, playing, quit) = (tasks.game.clone(), tasks.playing.clone(), tasks.quit_after_game.clone());
    std::thread::spawn(move || {
        window_while_playing(&app, true);
        let result = horizon::play(&game, &emit);
        *playing.lock().unwrap() = false;
        if quit.load(Ordering::Relaxed) {
            return app.exit(0);
        }
        window_while_playing(&app, false);
        let _ = app.emit("game-exit", result.err());
    });
    Ok(())
}

#[tauri::command(async)]
fn update_check() -> Result<Option<update::Update>, String> {
    update::check()
}

/// Downloads the newest launcher in place of this one and starts it.
#[tauri::command(async)]
fn update_install(app: AppHandle, tasks: State<Tasks>) -> Result<(), String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Обновить лаунчер можно после выхода из игры".into());
    }
    horizon::reset_cancel();
    let percent = std::cell::Cell::new(u64::MAX);
    let file = update::install(|done, total| {
        let now = done * 100 / total.max(1);
        if percent.replace(now) != now {
            let _ = app.emit("update-progress", now);
        }
    })?;
    let mut command = std::process::Command::new(&file);
    // The AppImage's GTK hook keeps an APPDIR it is given: the old one's, unmounted when this one exits.
    command.env_remove("APPDIR");
    command.spawn().map_err(|e| format!("{}: {e}", file.display()))?;
    app.exit(0);
    Ok(())
}

#[tauri::command(async)]
fn server_status() -> Result<ping::ServerStatus, String> {
    ping::ping(&config::Settings::load().server())
}

#[tauri::command]
fn settings_get() -> config::Settings {
    config::Settings::load()
}

#[tauri::command]
fn settings_set(mut settings: config::Settings) -> Result<(), String> {
    // Only `storage_set` changes it: it moves the files first.
    settings.storage = config::Settings::load().storage;
    settings.save()
}

/// Folders of the heavy part, the ones `storage_set` moves.
const STORAGE_PARTS: [&str; 2] = ["horizon", "game"];

/// Moves the game and the CLI's data to `<folder>/wcraft-launcher` (empty `folder`: back to the launcher's own
/// folder) and remembers it. Returns the new location, empty for the default one.
#[tauri::command(async)]
fn storage_set(folder: String, tasks: State<Tasks>) -> Result<String, String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Сначала закройте игру".into());
    }
    if tasks.task.lock().unwrap().is_some() || tasks.game.lock().unwrap().is_some() {
        return Err("Дождитесь окончания установки".into());
    }
    let folder = folder.trim();
    let (from, to) = (config::storage_dir(), match folder {
        "" => config::data_dir(),
        f => std::path::PathBuf::from(f).join("wcraft-launcher"),
    });
    let same = |a: &std::path::Path, b: &std::path::Path| a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b) || a == b;
    if same(&from, &to) {
        return Ok(config::Settings::load().storage);
    }
    if to.starts_with(&from) {
        return Err("Нельзя переносить внутрь текущей папки с игрой".into());
    }
    std::fs::create_dir_all(&to).map_err(|e| format!("Не удалось создать {}: {e}", to.display()))?;
    let mut moved: Vec<&str> = Vec::new();
    for part in STORAGE_PARTS {
        if let Err(e) = move_dir(&from.join(part), &to.join(part)) {
            for done in moved {
                let _ = move_dir(&to.join(done), &from.join(done));
            }
            return Err(format!("Не удалось перенести {part}: {e}"));
        }
        moved.push(part);
    }
    let mut settings = config::Settings::load();
    settings.storage = if folder.is_empty() { String::new() } else { to.to_string_lossy().into_owned() };
    settings.save()?;
    // The old place is ours only when it was a folder `storage_set` made.
    if from != config::data_dir() {
        let _ = std::fs::remove_dir(&from);
    }
    Ok(settings.storage)
}

/// Renames, or copies and removes when `to` is on another disk. Leaves nothing half-done in `to` on failure.
fn move_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if to.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} уже существует", to.display())));
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if let Err(e) = copy_tree(from, to) {
        let _ = std::fs::remove_dir_all(to);
        return Err(e);
    }
    std::fs::remove_dir_all(from)
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&src, &dst)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
            #[cfg(windows)]
            std::fs::copy(&src, &dst).map(|_| ())?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

#[tauri::command(async)]
fn skins_list() -> skins::Library {
    skins::library()
}

#[tauri::command(async)]
fn skins_add(name: String, model: String, png: Vec<u8>) -> Result<String, String> {
    let result = skins::add(&name, &model, &png)?;
    skins::publish_active();
    Ok(result)
}

/// The server's players' published skins by name.
#[tauri::command(async)]
fn skins_players() -> std::collections::BTreeMap<String, skins::Skin> {
    skins::players()
}

/// A page of the online skin catalog.
#[tauri::command(async)]
fn skins_catalog(query: String, page: u64, tag: String, sort: String) -> Result<skins::CatalogPage, String> {
    skins::catalog_search(&query, page, &tag, &sort)
}

/// A catalog skin's PNG (data URL), shown before it is added.
#[tauri::command(async)]
fn skins_catalog_png(id: String) -> Result<String, String> {
    skins::catalog_png(&id)
}

#[tauri::command(async)]
fn skins_catalog_install(id: String, title: String, model: String) -> Result<String, String> {
    let result = skins::catalog_install(&id, &title, &model)?;
    skins::publish_active();
    Ok(result)
}

#[tauri::command(async)]
fn skins_remove(id: String) -> Result<(), String> {
    skins::remove(&id)
}

#[tauri::command(async)]
fn skins_select(id: String) -> Result<(), String> {
    skins::select(&id)?;
    skins::publish_active();
    Ok(())
}

#[tauri::command(async)]
fn skins_set_model(id: String, model: String) -> Result<(), String> {
    skins::set_model(&id, &model)?;
    skins::publish_active();
    Ok(())
}

/// Opens the sign-in page again, for a player who closed it.
#[tauri::command(async)]
fn open_link(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Не ссылка".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn open_data_dir(app: AppHandle) -> Result<(), String> {
    let dir = config::data_dir();
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn storage_open(app: AppHandle) -> Result<(), String> {
    app.opener().open_path(config::storage_dir().to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Removes everything the launcher put on this computer: its folder (the CLI, Horizon, the game,
/// accounts, skins, settings) and the webview's own data, then quits. The program itself stays:
/// the player deletes the file or runs the uninstaller.
#[tauri::command(async)]
fn wipe_data(app: AppHandle, tasks: State<Tasks>) -> Result<(), String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Сначала закройте игру".into());
    }
    horizon::cancel(&tasks.task);
    let data = config::data_dir();
    let storage = config::storage_dir();
    if storage != data {
        for part in STORAGE_PARTS {
            remove_dir(&storage.join(part)).map_err(|e| format!("Не удалось удалить {}: {e}", storage.display()))?;
        }
        let _ = std::fs::remove_dir(&storage);
    }
    remove_dir(&data).map_err(|e| format!("Не удалось удалить {}: {e}", data.display()))?;
    let path = app.path();
    let webview: Vec<_> = [path.app_data_dir(), path.app_local_data_dir(), path.app_cache_dir(), path.app_config_dir()]
        .into_iter()
        .flatten()
        .filter(|d| d.exists())
        .collect();
    #[cfg(windows)]
    {
        // WebView2 holds its folder until we exit: a detached shell removes it a moment later.
        use std::os::windows::process::CommandExt;
        let mut script = String::from("ping -n 3 127.0.0.1 >nul");
        for dir in &webview {
            script.push_str(&format!(" & rmdir /s /q \"{}\"", dir.display()));
        }
        let _ = std::process::Command::new("cmd")
            .args(["/c", &script])
            .creation_flags(0x0800_0000 | 0x0000_0008) // CREATE_NO_WINDOW | DETACHED_PROCESS
            .spawn();
    }
    #[cfg(not(windows))]
    for dir in &webview {
        let _ = remove_dir(dir);
    }
    app.exit(0);
    Ok(())
}

/// The game reads its shader pack when it starts: changing it while it runs would not show.
fn not_playing(tasks: &State<Tasks>) -> Result<(), String> {
    match *tasks.playing.lock().unwrap() {
        true => Err("Шейдеры и ресурспаки меняются, пока игра закрыта".into()),
        false => Ok(()),
    }
}

#[tauri::command(async)]
fn shaders_list() -> Result<Vec<shaders::Shader>, String> {
    shaders::list()
}

#[tauri::command(async)]
fn shaders_install(id: String, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    shaders::install(&id)
}

#[tauri::command(async)]
fn shaders_update(id: String, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    shaders::update(&id)
}

/// Turns a pack on by its file, or all off with none.
#[tauri::command(async)]
fn shaders_set(file: Option<String>, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    shaders::set(file.as_deref())
}

#[tauri::command(async)]
fn shaders_remove(id: String, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    shaders::remove(&id)
}

#[tauri::command(async)]
fn modrinth_search(kind: String, query: String, offset: usize) -> Result<Vec<shaders::Shader>, String> {
    modrinth::search(&kind, &query, offset)
}

#[tauri::command(async)]
fn modrinth_gallery(slug: String) -> Result<Vec<String>, String> {
    modrinth::gallery(&slug)
}

#[tauri::command(async)]
fn modrinth_install(kind: String, slug: String, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    modrinth::install(&kind, &slug)
}

#[tauri::command(async)]
fn resources_list() -> Result<Vec<shaders::Shader>, String> {
    resources::list()
}

#[tauri::command(async)]
fn resources_set(file: String, on: bool, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    resources::set(&file, on)
}

#[tauri::command(async)]
fn resources_remove(file: String, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    resources::remove(&file)
}

/// As `shaders_add`: a `.zip` resource pack's bytes as the raw body.
#[tauri::command(async)]
fn resources_add(request: tauri::ipc::Request<'_>, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нет файла".into());
    };
    let name = request.headers().get("x-name").and_then(|v| v.to_str().ok()).unwrap_or_default();
    resources::add(&percent_decode(name), bytes)
}

/// A pack file the player picked: its bytes as the raw body, its name URI-encoded in a header.
#[tauri::command(async)]
fn shaders_add(request: tauri::ipc::Request<'_>, tasks: State<'_, Tasks>) -> Result<(), String> {
    not_playing(&tasks)?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нет файла".into());
    };
    let name = request.headers().get("x-name").and_then(|v| v.to_str().ok()).unwrap_or_default();
    shaders::add(&percent_decode(name), bytes)
}

#[tauri::command(async)]
fn mods_state() -> mods::State {
    mods::state(horizon::is_installed)
}

/// A mod file the player picked: its bytes as the raw body, its name URI-encoded in a header. Gives the mod's id.
#[tauri::command(async)]
fn mods_add(request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Нет файла".into());
    };
    let name = request.headers().get("x-name").and_then(|v| v.to_str().ok()).unwrap_or_default();
    mods::add(&percent_decode(name), bytes)
}

#[tauri::command(async)]
fn mods_set(id: String, on: bool) -> Result<(), String> {
    mods::set(&id, on)
}

#[tauri::command(async)]
fn mods_remove(id: String) -> Result<(), String> {
    mods::remove(&id)
}

#[tauri::command(async)]
fn mods_open_dir(app: AppHandle) -> Result<(), String> {
    let dir = mods::folder()?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Compiles the game with WCraft's mods and the player's; `cancel` stops it.
#[tauri::command(async)]
fn mods_build(app: AppHandle, tasks: State<Tasks>) -> Result<(), String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Сначала закройте игру".into());
    }
    horizon::reset_cancel();
    mods::build(&tasks.task, &emitter(&app))
}

#[tauri::command(async)]
fn mods_clean(tasks: State<Tasks>) -> Result<(), String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Сначала закройте игру".into());
    }
    mods::clean()
}

/// Starts the launcher again (the experimental mode changes what it shows). A dev run only reloads the window.
#[tauri::command(async)]
fn restart(app: AppHandle, tasks: State<Tasks>) -> Result<(), String> {
    if *tasks.playing.lock().unwrap() {
        return Err("Лаунчер перезапустится после выхода из игры".into());
    }
    let Some(file) = update::target() else {
        let window = app.get_webview_window("main").ok_or("Нет окна")?;
        return window.eval("location.reload()").map_err(|e| e.to_string());
    };
    let mut command = std::process::Command::new(&file);
    // As in `update_install`: the AppImage's GTK hook would keep this run's APPDIR, unmounted once it exits.
    command.env_remove("APPDIR");
    command.spawn().map_err(|e| format!("{}: {e}", file.display()))?;
    app.exit(0);
    Ok(())
}

/// `encodeURIComponent` undone.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match (b[i], s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tauri::command(async)]
fn shaders_open_dir(app: AppHandle) -> Result<(), String> {
    let dir = shaders::folder()?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn resources_open_dir(app: AppHandle) -> Result<(), String> {
    let dir = resources::folder()?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

/// Bytes the launcher's folder takes: everything `wipe_data` frees, but the webview's few MB.
#[tauri::command(async)]
fn data_size() -> u64 {
    let (data, storage) = (config::data_dir(), config::storage_dir());
    horizon::dir_size(&data) + if storage != data { horizon::dir_size(&storage) } else { 0 }
}

fn remove_dir(dir: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// The single-file Windows build has no installer to bootstrap WebView2; without it Tauri
/// would panic before any window, so say what is missing and offer Microsoft's bootstrapper.
#[cfg(windows)]
fn webview_missing() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO};
    if tauri::webview_version().is_ok() {
        return false;
    }
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let text = wide("Для WCraft нужен Microsoft Edge WebView2 Runtime, а на этом компьютере его нет.\n\nСкачать установщик от Microsoft? После установки запустите WCraft снова.");
    let title = wide("WCraft");
    let answer = unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_YESNO | MB_ICONWARNING) };
    if answer == IDYES {
        let _ = tauri_plugin_opener::open_url("https://go.microsoft.com/fwlink/p/?LinkId=2124703", None::<&str>);
    }
    true
}

pub fn run() {
    #[cfg(windows)]
    if webview_missing() {
        return;
    }
    update::clean_up();
    applog::start();
    // In the background: the window starts on the kept pack, and Play asks again anyway.
    std::thread::spawn(update::refresh_pack);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Tasks::default())
        .invoke_handler(tauri::generate_handler![
            status,
            auth,
            account_offline,
            account_select,
            account_rename,
            account_password,
            account_logout_others,
            account_delete,
            account_info,
            account_remove,
            sign_in,
            sign_out,
            cancel,
            play,
            server_status,
            update_check,
            update_install,
            settings_get,
            settings_set,
            skins_list,
            skins_add,
            skins_catalog,
            skins_catalog_install,
            skins_catalog_png,
            skins_remove,
            skins_select,
            skins_set_model,
            skins_players,
            shaders_list,
            shaders_install,
            shaders_update,
            shaders_set,
            shaders_remove,
            shaders_add,
            shaders_open_dir,
            modrinth_search,
            modrinth_gallery,
            modrinth_install,
            resources_list,
            resources_set,
            resources_remove,
            resources_add,
            resources_open_dir,
            open_link,
            open_data_dir,
            storage_set,
            storage_open,
            wipe_data,
            data_size,
            mods_state,
            mods_add,
            mods_set,
            mods_remove,
            mods_open_dir,
            mods_build,
            mods_clean,
            restart
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let tasks = window.state::<Tasks>();
                if *tasks.playing.lock().unwrap() {
                    api.prevent_close();
                    tasks.quit_after_game.store(true, Ordering::Relaxed);
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to start WCraft launcher");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_dir_moves_and_refuses_existing_target() {
        let root = std::env::temp_dir().join(format!("wcraft-move-{}", std::process::id()));
        let (from, to) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(from.join("x/y")).unwrap();
        std::fs::write(from.join("x/y/f.txt"), "hi").unwrap();
        move_dir(&from, &to).unwrap();
        assert!(!from.exists());
        assert_eq!(std::fs::read_to_string(to.join("x/y/f.txt")).unwrap(), "hi");
        std::fs::create_dir_all(&from).unwrap();
        assert!(move_dir(&from, &to).is_err());
        let copy = root.join("c");
        copy_tree(&to, &copy).unwrap();
        assert_eq!(std::fs::read_to_string(copy.join("x/y/f.txt")).unwrap(), "hi");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Pings wcraft.json's server through its tunnel: `cargo test -- --ignored live_server`.
    #[test]
    #[ignore]
    fn live_server_answers_ping() {
        let status = ping::ping(&config::pack().server).unwrap();
        assert_eq!(status.version, "26.3");
    }

    #[test]
    fn ping_reads_a_status_response() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 256];
            let _ = s.read(&mut buf);
            let json = br#"{"version":{"name":"26.3"},"players":{"online":3,"max":20,"sample":[{"name":"Wahha"}]}}"#;
            let mut body = vec![0x00, json.len() as u8];
            body.extend_from_slice(json);
            let mut out = vec![body.len() as u8];
            out.extend(body);
            s.write_all(&out).unwrap();
        });
        let status = ping::ping(&format!("127.0.0.1:{port}")).unwrap();
        assert_eq!((status.online, status.max), (3, 20));
        assert_eq!(status.players, ["Wahha"]);
        assert_eq!(status.version, "26.3");
    }

    pub(super) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn sidecar_is_found() {
        let _guard = TEST_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("wcraft-empty-{}", std::process::id()));
        std::env::set_var("WCRAFT_HOME", &home);
        let cli = horizon::cli();
        assert!(cli.is_some(), "Sidecar should be found even when data dir is empty");
        // Built with WCRAFT_EMBED_CLI: unpacked into the data folder and runnable.
        #[cfg(embedded_cli)]
        {
            let cli = cli.unwrap();
            assert!(cli.starts_with(home.join("cli")), "{}", cli.display());
            assert!(horizon::cli() == Some(cli.clone()), "second lookup reuses the unpacked file");
            let out = std::process::Command::new(&cli).args(["account", "--help"]).output().unwrap();
            assert!(String::from_utf8_lossy(&out.stdout).contains("offline"));
        }
        let _ = std::fs::remove_dir_all(&home);
        std::env::remove_var("WCRAFT_HOME");
    }

    #[test]
    #[ignore = "changes the real CLI's accounts and drops the selected one"]
    fn offline_accounts_work() {
        let _guard = TEST_LOCK.lock().unwrap();
        if horizon::cli().is_none() {
            return;
        }
        let acc = horizon::account_offline("WahhaTest").unwrap();
        assert_eq!(acc.name, "WahhaTest");
        assert!(acc.offline);
        let st = horizon::status();
        assert!(st.accounts.iter().any(|a| a.name == "WahhaTest" && a.offline));
        horizon::account_remove("WahhaTest", true).unwrap();
    }

    #[test]
    fn skins_are_kept_checked_and_removed() {
        let _guard = TEST_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("wcraft-test-{}", std::process::id()));
        std::env::set_var("WCRAFT_HOME", &home);
        assert!(skins::add("x", "classic", b"not a png").is_err());
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend([0, 0, 0, 64, 0, 0, 0, 64, 8, 6, 0, 0, 0]);
        let id = skins::add("Мой скин", "slim", &png).unwrap();
        let lib = skins::library();
        assert_eq!(lib.active.as_deref(), Some(id.as_str()));
        assert_eq!((lib.saved.len(), lib.saved[0].model.as_str()), (1, "slim"));
        skins::set_model(&id, "classic").unwrap();
        assert_eq!(skins::library().saved[0].model, "classic");
        skins::remove(&id).unwrap();
        assert!(skins::library().saved.is_empty());
        // The default skins ship with the launcher, before any install.
        let defaults = skins::library().defaults;
        assert_eq!(defaults.len(), 9);
        assert!(defaults.iter().any(|s| s.id == "default:alex" && s.model == "slim"));
        let _ = std::fs::remove_dir_all(home);
        std::env::remove_var("WCRAFT_HOME");
    }

    /// Needs backend/auth running at wcraft.json's `auth`, with a throwaway database and server folder:
    #[test]
    fn skin_cache_names_are_the_games() {
        // SHA-1 of the hash's UTF-16LE code units, as Horizon's `skins::skin_location` names the file.
        let hash = "99391ed9dff9581ec88daf23b9b028adc9ac91f52889edcff1c982d0923c0cff";
        assert_eq!(skins::cache_name(hash), "377e4139e28a332a82ad790d54b14e84f51b3029");
    }

    /// Against an accounts service with skins:
    /// `WCRAFT_AUTH_URL=http://127.0.0.1:8095 cargo test -- --ignored skins_round_trip`.
    #[test]
    #[ignore]
    fn skins_round_trip() {
        use sha2::{Digest, Sha256};
        const ALEX: &[u8] = include_bytes!("../skins/alex.png");
        let _guard = TEST_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("wcraft-skins-{}", std::process::id()));
        std::env::set_var("WCRAFT_HOME", &home);
        let name = format!("S{}", std::process::id() % 1_000_000);
        auth::authenticate(true, &name, "secret1").unwrap();
        let id = skins::add("Alex", "slim", ALEX).unwrap();
        skins::publish(&name).unwrap();
        let list = skins::sync().unwrap();
        let mine = list.iter().find(|p| p.name == name).unwrap();
        let hash: String = Sha256::digest(ALEX).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!((mine.hash.as_str(), mine.model.as_str()), (hash.as_str(), "slim"));
        let game = home.join("game");
        skins::install(&list, &game);
        let file = skins::cache_name(&hash);
        assert_eq!(std::fs::read(game.join("assets/skins").join(&file[..2]).join(&file)).unwrap(), ALEX);
        assert!(skins::players()[&name].url.starts_with("data:image/png;base64,"));
        // No skin chosen: the server forgets the published one.
        skins::remove(&id).unwrap();
        skins::publish(&name).unwrap();
        assert!(!skins::sync().unwrap().iter().any(|p| p.name == name));
        auth::logout(&name).unwrap();
        let _ = std::fs::remove_dir_all(home);
        std::env::remove_var("WCRAFT_HOME");
    }

    /// `cargo test -- --ignored auth_round_trip`.
    #[test]
    #[ignore]
    fn auth_round_trip() {
        let _guard = TEST_LOCK.lock().unwrap();
        let home = std::env::temp_dir().join(format!("wcraft-auth-{}", std::process::id()));
        std::env::set_var("WCRAFT_HOME", &home);
        let name = format!("T{}", std::process::id() % 1_000_000);
        assert_eq!(auth::authenticate(true, &name, "secret1").unwrap(), name);
        assert_eq!(auth::authenticate(true, &name, "secret1").unwrap_err(), "Этот ник уже занят");
        assert_eq!(auth::authenticate(false, &name.to_lowercase(), "secret1").unwrap(), name);
        assert_eq!(auth::logged_in(), [name.clone()]);
        auth::join(&name).unwrap();
        assert_eq!(auth::authenticate(false, &name, "wrong!!").unwrap_err(), "Неверный ник или пароль");
        auth::logout(&name).unwrap();
        assert!(auth::join(&name).unwrap_err().starts_with("Войдите в аккаунт"));
        let _ = std::fs::remove_dir_all(home);
        std::env::remove_var("WCRAFT_HOME");
    }

    fn printing() -> Emit {
        Arc::new(|event, payload| println!("[{event}] {payload}"))
    }

    /// Downloads the real CLI from GitHub (about 30 MB): `cargo test -- --ignored bootstrap`.
    #[test]
    #[ignore]
    fn bootstrap_downloads_the_cli() {
        let home = std::env::temp_dir().join(format!("wcraft-boot-{}", std::process::id()));
        std::env::set_var("WCRAFT_HOME", &home);
        bootstrap::install(&printing()).unwrap();
        let exe = horizon::bin_dir();
        let out = std::process::Command::new("find").arg(&exe).args(["-name", horizon::EXE]).output().unwrap();
        assert!(!out.stdout.is_empty(), "no CLI under {}", exe.display());
        let _ = std::fs::remove_dir_all(home);
    }

    /// Runs setup against this computer's own Horizon: `cargo test -- --ignored setup`.
    #[test]
    #[ignore]
    fn setup_makes_this_computer_ready() {
        horizon::setup(&Slot::default(), &printing()).unwrap();
        let status = horizon::status();
        assert!(status.installed && status.prepared && status.instance);
    }
}
