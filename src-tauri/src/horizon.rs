//! Everything the game needs goes through the `horizon-launcher` CLI: accounts, installs,
//! preparing and playing. This module finds it, runs it and reports what is ready.
use crate::{bootstrap, config, update};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const EXE: &str = if cfg!(windows) { "horizon-launcher.exe" } else { "horizon-launcher" };

/// Sends an event to the window: `log` (a line), `step` (what setup is doing), `progress`, `signin`.
pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;
/// The running child of a task, so it can be cancelled.
pub type Slot = Mutex<Option<Child>>;

/// Runs a blocking call (a connection, a request) on its own thread and waits for it in short steps,
/// so a cancel is felt at once; the abandoned call finishes on its own and is dropped.
pub fn interruptible<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    loop {
        check_cancel()?;
        match rx.recv_timeout(Duration::from_millis(30)) {
            Ok(v) => return Ok(v),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("Загрузка оборвалась".into()),
        }
    }
}

/// One read of a download whose socket has a short read timeout: timeouts only give a chance to
/// notice a cancel, and a stall of half a minute is an error.
pub fn read_chunk(reader: &mut impl Read, buf: &mut [u8], last_data: &mut std::time::Instant) -> Result<usize, String> {
    loop {
        check_cancel()?;
        match reader.read(buf) {
            Ok(n) => {
                *last_data = std::time::Instant::now();
                return Ok(n);
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted) => {
                if last_data.elapsed() > Duration::from_secs(30) {
                    return Err("Загрузка оборвалась".into());
                }
            }
            Err(_) => return Err("Загрузка оборвалась".into()),
        }
    }
}

/// `json` for the steps of a setup: the child is killed as soon as the task is cancelled.
pub fn json_task(args: &[&str]) -> Result<Value, String> {
    let mut child = command(args)?
        .arg("--json")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let read = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut v = Vec::new();
            let _ = r.read_to_end(&mut v);
            v
        })
    };
    let out = read(Box::new(child.stdout.take().unwrap()));
    let err = read(Box::new(child.stderr.take().unwrap()));
    let status = loop {
        if check_cancel().is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Отменено".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if !status.success() {
        return Err(last_line(&err));
    }
    Ok(serde_json::from_slice(&out).unwrap_or(Value::Null))
}

/// Set by `cancel`: steps that run no child (downloads, the gaps between commands) watch it.
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn reset_cancel() {
    CANCELLED.store(false, std::sync::atomic::Ordering::SeqCst);
}

pub fn check_cancel() -> Result<(), String> {
    if CANCELLED.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Отменено".into());
    }
    Ok(())
}

pub fn bin_dir() -> PathBuf {
    config::data_dir().join("bin")
}

pub fn find_in(dir: &Path, depth: u8) -> Option<PathBuf> {
    let direct = dir.join(EXE);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .find_map(|e| find_in(&e.path(), depth - 1))
}

fn is_offline_capable(path: &Path) -> bool {
    let mut c = Command::new(path);
    c.args(["account", "--help"]).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.output()
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.contains("offline")
        })
        .unwrap_or(false)
}

fn sidecar_path() -> Option<PathBuf> {
    // 1. Next to the running executable (in installed AppImage, .deb, Windows .exe)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidates = [
                dir.join(EXE),
                dir.join("bin").join(EXE),
                dir.join("resources").join(EXE),
                dir.join("../Resources").join(EXE),
            ];
            for path in candidates {
                if path.is_file() && is_offline_capable(&path) {
                    return Some(path);
                }
            }
        }
    }

    // 2. Development sidecar in src-tauri/binaries/
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let binaries_dir = manifest_dir.join("binaries");
    let target = env!("TARGET");
    let target_bin = if cfg!(windows) {
        binaries_dir.join(format!("horizon-launcher-{target}.exe"))
    } else {
        binaries_dir.join(format!("horizon-launcher-{target}"))
    };
    if target_bin.is_file() && is_offline_capable(&target_bin) {
        return Some(target_bin);
    }

    if let Ok(entries) = std::fs::read_dir(&binaries_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.starts_with("horizon-launcher") && is_offline_capable(&path) {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// The CLI baked into this executable (`WCRAFT_EMBED_CLI` at build time), unpacked once into
/// `<data>/cli/<hash>/`; older unpacked versions are removed.
#[cfg(embedded_cli)]
fn embedded_cli() -> Option<PathBuf> {
    static UNPACK: Mutex<()> = Mutex::new(());
    let _guard = UNPACK.lock().ok()?;
    let root = config::data_dir().join("cli");
    let dir = root.join(env!("WCRAFT_CLI_HASH"));
    let path = dir.join(EXE);
    if path.is_file() {
        return Some(path);
    }
    std::fs::create_dir_all(&dir).ok()?;
    let part = dir.join(format!("{EXE}.part"));
    let gz: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cli.gz"));
    let mut out = std::fs::File::create(&part).ok()?;
    std::io::copy(&mut flate2::read::GzDecoder::new(gz), &mut out).ok()?;
    drop(out);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755)).ok()?;
    }
    std::fs::rename(&part, &path).ok()?;
    for entry in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        if entry.path() != dir {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    Some(path)
}

#[cfg(not(embedded_cli))]
fn embedded_cli() -> Option<PathBuf> {
    None
}

/// 1. OTA update in data dir (~/.local/share/wcraft-launcher/bin)
/// 2. CLI embedded in the executable (single-file Windows build)
/// 3. Bundled sidecar inside the application package (or src-tauri/binaries in dev)
/// 4. PATH / ~/.local/bin (only if offline-capable)
pub fn cli() -> Option<PathBuf> {
    find_in(&bin_dir(), 2)
        .filter(|p| is_offline_capable(p))
        .or_else(embedded_cli)
        .or_else(sidecar_path)
        .or_else(|| {
            std::env::var_os("PATH")
                .and_then(|p| std::env::split_paths(&p).map(|d| d.join(EXE)).find(|f| f.is_file()))
                .filter(|p| is_offline_capable(p))
        })
        .or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/bin").join(EXE))
                .filter(|f| f.is_file())
                .filter(|p| is_offline_capable(p))
        })
}

fn command(args: &[&str]) -> Result<Command, String> {
    let cli = cli().ok_or("Horizon Launcher не установлен")?;
    let mut c = Command::new(cli);
    c.args(args)
        .stdin(Stdio::null())
        .env("HORIZON_LAUNCHER_HOME", home())
        .env("HORIZON_HOME", game_home())
        // Microsoft tokens in the data folder, not the system keychain: wiping removes them.
        .env("HORIZON_NO_KEYCHAIN", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console window for a console-subsystem CLI. The CLI is a GUI program, so it has no
        // console at all and hides the consoles of what it runs itself (`console::in_terminal` in horizon-cli).
        // CREATE_NEW_PROCESS_GROUP: the group ignores Ctrl+C, so a stray console event cannot kill a step
        // with 0xc000013a (STATUS_CONTROL_C_EXIT), as happened to `prepare`.
        c.creation_flags(0x0800_0000 | 0x0000_0200);
    }
    Ok(c)
}

fn last_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return "Команда не удалась".to_string();
    }
    if let Some(&last) = lines.last() {
        if last.contains("--help") && lines.len() > 1 {
            if let Some(err_line) = lines.iter().rev().find(|l| l.starts_with("error:")) {
                return err_line.strip_prefix("error:").unwrap_or(err_line).trim().to_string();
            }
            return lines[lines.len() - 2].to_string();
        }
        return last.to_string();
    }
    "Команда не удалась".to_string()
}

/// Runs a quick command and parses its `--json` output.
pub fn json(args: &[&str]) -> Result<Value, String> {
    let out = command(args)?.arg("--json").output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(last_line(&out.stderr));
    }
    Ok(serde_json::from_slice(&out.stdout).unwrap_or(Value::Null))
}

/// "To sign in, open LINK in a browser and enter the code CODE"
fn sign_in_code(line: &str) -> Option<Value> {
    let (_, rest) = line.split_once(", open ")?;
    let (link, code) = rest.split_once(" in a browser and enter the code ")?;
    Some(json!({ "link": link.trim(), "code": code.trim() }))
}

fn pump(mut pipe: impl Read + Send + 'static, emit: Emit, last: Arc<Mutex<String>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line = Vec::new();
        loop {
            let n = match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            for &b in &buf[..n] {
                if b != b'\n' && b != b'\r' {
                    line.push(b);
                    continue;
                }
                let text = String::from_utf8_lossy(&line).trim().to_string();
                line.clear();
                if text.is_empty() {
                    continue;
                }
                if let Some(code) = sign_in_code(&text) {
                    emit("signin", code);
                }
                emit("log", Value::String(text.clone()));
                *last.lock().unwrap() = text;
            }
        }
    })
}

/// Runs a long command, its output going to the window line by line. The child sits in `slot`
/// meanwhile: taking it out (see `cancel`) ends the run.
pub fn run(args: &[&str], slot: &Slot, emit: &Emit) -> Result<(), String> {
    run_with(args, &[], slot, emit).map(drop)
}

/// `run`, giving the last line the command printed.
pub fn run_last(args: &[&str], slot: &Slot, emit: &Emit) -> Result<String, String> {
    run_with(args, &[], slot, emit)
}

/// `run` with environment variables, which the game inherits from horizon-launcher; gives the last line printed.
fn run_with(args: &[&str], env: &[(&str, String)], slot: &Slot, emit: &Emit) -> Result<String, String> {
    emit("log", Value::String(format!("> horizon-launcher {}", args.join(" "))));
    let mut child = command(args)?
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let last = Arc::new(Mutex::new(String::new()));
    let readers = [
        pump(child.stdout.take().unwrap(), emit.clone(), last.clone()),
        pump(child.stderr.take().unwrap(), emit.clone(), last.clone()),
    ];
    *slot.lock().unwrap() = Some(child);
    let status = loop {
        std::thread::sleep(Duration::from_millis(30));
        let mut guard = slot.lock().unwrap();
        let Some(child) = guard.as_mut() else {
            return Err("Отменено".into());
        };
        if check_cancel().is_err() {
            let _ = child.kill();
            let _ = child.wait();
            *guard = None;
            return Err("Отменено".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            *guard = None;
            break status;
        }
    };
    for r in readers {
        let _ = r.join();
    }
    let last = last.lock().unwrap().clone();
    if status.success() {
        return Ok(last);
    }
    Err(if last.is_empty() { format!("Команда завершилась с кодом {status}") } else { last })
}

pub fn cancel(slot: &Slot) {
    CANCELLED.store(true, std::sync::atomic::Ordering::SeqCst);
    if let Some(mut child) = slot.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The CLI's data folder, where installed versions and their prepared resources are. It is
/// inside ours, so a player's own Horizon is never touched and wiping our folder removes it all.
fn home() -> PathBuf {
    config::storage_dir().join("horizon")
}

pub fn launcher_home() -> PathBuf {
    home()
}

/// Where the game keeps instances, options and logs.
pub fn game_home() -> PathBuf {
    config::storage_dir().join("game")
}

/// The prepared resources of a version (`…/horizon-client/26.3`), once `prepare` made them.
pub fn prepared_dir(tag: &str) -> Option<PathBuf> {
    let root = home().join("versions").join(tag).join(".local/horizon-client");
    // `horizon-prepare.json` is what a finished prepare leaves; `assets` appears halfway, so a crashed run has it too.
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.join("assets").is_dir() && p.join("horizon-prepare.json").is_file())
}

pub fn offline_uuid(name: &str) -> String {
    let mut hash = md5::compute(format!("OfflinePlayer:{name}").as_bytes()).0;
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    let h: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

#[derive(Clone, Serialize, serde::Deserialize)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    pub offline: bool,
}

#[derive(Serialize)]
pub struct Status {
    /// The CLI's version, or None when it is not on this computer yet.
    pub cli: Option<String>,
    pub account: Option<Account>,
    pub accounts: Vec<Account>,
    /// In Horizon-Offline-Launcher gate is open, so offline accounts can always play.
    pub owner: bool,
    pub installed: bool,
    pub prepared: bool,
    pub instance: bool,
    pub pack: config::Pack,
    pub server: String,
}

/// The game folder of an instance: where it runs and keeps its skin cache.
pub fn instance_home(name: &str) -> Option<PathBuf> {
    let list = json(&["instance", "list"]).ok()?;
    let home = list.as_array()?.iter().find(|i| i["name"] == name)?["home"].as_str()?.to_owned();
    Some(PathBuf::from(home))
}

fn has_instance(name: &str) -> bool {
    json(&["instance", "list"])
        .ok()
        .and_then(|v| v.as_array().map(|a| a.iter().any(|i| i["name"] == name)))
        .unwrap_or(false)
}

fn installed_versions() -> Vec<String> {
    json(&["versions", "--installed"])
        .ok()
        .and_then(|v| v["installed"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect()))
        .unwrap_or_default()
}

pub fn is_installed(tag: &str) -> bool {
    installed_versions().iter().any(|t| t == tag)
}

pub fn status() -> Status {
    let pack = config::pack();
    let server = config::Settings::load().server();
    let version = command(&["--version"]).ok().and_then(|mut c| c.output().ok()).map(|o| {
        let text = String::from_utf8_lossy(&o.stdout);
        text.split_whitespace().last().unwrap_or("").to_string()
    });
    if version.is_none() {
        return Status {
            cli: None,
            account: None,
            accounts: Vec::new(),
            owner: true,
            installed: false,
            prepared: false,
            instance: false,
            pack,
            server,
        };
    }
    let accounts_val = json(&["account", "list"]).unwrap_or(Value::Null);
    let mut accounts = Vec::new();
    let mut selected = None;

    // 1. Offline accounts
    if let Some(offlines) = accounts_val["offline"].as_array() {
        for item in offlines {
            let name = item["name"].as_str().unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            let is_sel = item["selected"].as_bool().unwrap_or(false);
            let acc = Account {
                uuid: offline_uuid(&name),
                name,
                offline: true,
            };
            if is_sel {
                selected = Some(acc.clone());
            }
            accounts.push(acc);
        }
    }

    // 2. Microsoft accounts (if any exist)
    if let Some(ms_accounts) = accounts_val["accounts"].as_array() {
        for item in ms_accounts {
            let name = item["name"].as_str().unwrap_or("").trim().to_string();
            let uuid = item["uuid"].as_str().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let is_sel = item["selected"].as_bool().unwrap_or(false);
            let acc = Account {
                name,
                uuid,
                offline: false,
            };
            if is_sel && selected.is_none() {
                selected = Some(acc.clone());
            }
            accounts.push(acc);
        }
    }

    if selected.is_none() {
        selected = accounts.first().cloned();
    }

    Status {
        cli: version,
        account: selected,
        accounts,
        owner: true,
        installed: is_installed(&pack.horizon_version),
        prepared: prepared_dir(&pack.horizon_version).is_some(),
        instance: has_instance(&pack.instance),
        pack,
        server,
    }
}

/// Validate an offline nickname (3 to 16 ASCII letters, digits or underscore).
pub fn is_valid_offline_name(name: &str) -> bool {
    let count = name.chars().count();
    (3..=16).contains(&count) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Add an offline account and select it.
pub fn account_offline(name: &str) -> Result<Account, String> {
    let name = name.trim();
    if !is_valid_offline_name(name) {
        return Err("Имя должно быть от 3 до 16 символов (буквы, цифры или _)".into());
    }
    let out = command(&["account", "offline", name])?
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(last_line(&out.stderr));
    }
    Ok(Account {
        name: name.to_string(),
        uuid: offline_uuid(name),
        offline: true,
    })
}

/// Select an account by name.
pub fn account_select(name: &str) -> Result<(), String> {
    let out = command(&["account", "select", name])?
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(last_line(&out.stderr));
    }
    Ok(())
}

/// Remove/forget an account.
pub fn account_remove(name: &str, offline: bool) -> Result<(), String> {
    let cmd = if offline {
        vec!["account", "forget-offline", name]
    } else {
        vec!["account", "sign-out", name]
    };
    let out = command(&cmd)?.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(last_line(&out.stderr));
    }
    Ok(())
}

/// Apply a skin PNG to an offline name via `horizon-launcher account skin`.
pub fn account_set_skin(name: &str, png_path: &Path, model: &str) -> Result<(), String> {
    let path_str = png_path.to_str().ok_or("Некорректный путь к файлу скина")?;
    let out = command(&["account", "skin", name, path_str, "--model", model])?
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(last_line(&out.stderr));
    }
    Ok(())
}

/// Brings this computer to the point where Play works: the CLI, the pinned Horizon version,
/// its resources, our instance kept on that version. Each step is skipped when already done.
pub fn setup(slot: &Slot, emit: &Emit) -> Result<(), String> {
    let pack = config::pack();
    let tag = pack.horizon_version.as_str();
    let step = |text: &str| emit("step", Value::String(text.to_string()));
    if cli().is_none() {
        step("Скачиваю Horizon Launcher");
        bootstrap::install(emit)?;
    }
    check_cancel()?;
    // First install only: with a version already there, `install` also reuses its prepared resources.
    if let Some(base) = pack.base.as_ref().filter(|_| !is_installed(tag) && installed_versions().is_empty()) {
        step("Скачиваю Horizon");
        match install_from_service(base, tag, emit) {
            Err(e) if e == "Отменено" => return Err(e),
            Err(e) => emit("log", Value::String(format!("С нашего сервера не скачалось ({e}), беру у Horizon"))),
            Ok(()) => {}
        }
        emit("progress", Value::Null);
    }
    let installed = is_installed(tag);
    let _watch = (!installed || prepared_dir(tag).is_none()).then(|| watch(tag, emit));
    if !installed {
        step("Устанавливаю Horizon");
        run(&["install", tag], slot, emit)?;
    }
    if prepared_dir(tag).is_none() {
        step("Готовлю ресурсы игры");
        // In Horizon-Offline-Launcher, download works without Microsoft account.
        if let Err(first) = run(&["prepare", "--version", tag], slot, emit) {
            if first == "Отменено" {
                return Err(first);
            }
            step("Скачиваю ресурсы Minecraft");
            run(&["prepare", "--download", "--version", tag], slot, emit)?;
        }
    }
    check_cancel()?;
    if !has_instance(&pack.instance) {
        step("Создаю профиль WCraft");
        json_task(&["instance", "create", &pack.instance])?;
    }
    check_cancel()?;
    let official = match &pack.client {
        Some(client) => client_version(client, tag, emit)?,
        None => tag.to_string(),
    };
    // Experimental: the game built with the player's own mods, while it is built from the mods and pack there are now.
    let plays = match crate::mods::ready_build().filter(|t| is_installed(t)) {
        Some(built) => {
            emit("log", Value::String(format!("Играю со своими модами: {built}")));
            built
        }
        None => official.clone(),
    };
    json_task(&["instance", "version", &pack.instance, &plays])?;
    let kept = crate::mods::kept_build();
    // A version the pack moved away from only takes room (about a gigabyte and a half each).
    for old in installed_versions().into_iter().filter(|t| t != tag && *t != plays && *t != official && Some(t) != kept.as_ref()) {
        step("Удаляю старую версию игры");
        if let Err(e) = json(&["remove", &old]) {
            emit("log", Value::String(format!("Версия {old} не удалена: {e}")));
        }
    }
    Ok(())
}

/// Installs Horizon's own build of `tag` from the archive our service keeps (`Pack::base`), as `horizon-launcher install`
/// does it: the archive's one top folder becomes the version's folder, marked `.installed`. GitHub, where Horizon's
/// releases are, is slow or closed for some players; the archive's SHA-256 comes from the signed pack.
fn install_from_service(base: &update::Release, tag: &str, emit: &Emit) -> Result<(), String> {
    let versions = home().join("versions");
    let (partial, target) = (versions.join(format!(".{tag}.partial")), versions.join(tag));
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(&partial).map_err(|e| e.to_string())?;
    let archive = partial.join(&base.name);
    let progress = |done: u64, total: u64| emit("progress", json!({ "done": done, "total": total }));
    let result = update::download_client(base, &archive, &progress).and_then(|()| {
        check_cancel()?;
        emit("progress", Value::Null);
        emit("step", Value::String("Распаковываю Horizon".into()));
        place_version(&archive, &partial.join("unpacked"), &target)
    });
    let _ = std::fs::remove_dir_all(&partial);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&target);
    }
    result
}

fn place_version(archive: &Path, unpacked: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    zip::ZipArchive::new(file).and_then(|mut z| z.extract(unpacked)).map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(unpacked).map_err(|e| e.to_string())?;
    let entries: Vec<_> = std::fs::read_dir(unpacked).map_err(|e| e.to_string())?.flatten().collect();
    let top = match entries.as_slice() {
        [only] if only.path().is_dir() => only.path(),
        _ => unpacked.to_path_buf(),
    };
    let client = top.join(if cfg!(windows) { "horizon-client.exe" } else { "horizon-client" });
    if !client.exists() {
        return Err("В архиве нет игры".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&client, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_dir_all(target);
    std::fs::rename(&top, target).map_err(|e| e.to_string())?;
    // The mark the CLI counts a version as installed by, written last.
    std::fs::write(target.join(".installed"), "").map_err(|e| e.to_string())
}

/// The pack's game program (its mods compiled in, `scripts/release-pack.sh --client`) as a version on the game files
/// of `base`: downloaded and checked once, installed with `import-build`, its version kept in `client.json`.
fn client_version(client: &update::Release, base: &str, emit: &Emit) -> Result<String, String> {
    let record = config::data_dir().join("client.json");
    let kept = std::fs::read(&record).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let same = |k: &&Value| k["sha256"] == client.sha256.as_str() && k["base"] == base;
    if let Some(tag) = kept.as_ref().filter(same).and_then(|k| k["tag"].as_str()) {
        if is_installed(tag) {
            write_build_info(client, tag)?;
            return Ok(tag.to_string());
        }
    }
    emit("step", Value::String("Скачиваю сборку WCraft с модами".into()));
    let file = config::data_dir().join(&client.name);
    let path = file.to_str().ok_or("Некорректный путь к сборке")?;
    let progress = |done: u64, total: u64| emit("progress", json!({ "done": done, "total": total }));
    let result = update::download_client(client, &file, &progress).and_then(|()| {
        check_cancel()?;
        emit("progress", Value::Null);
        emit("step", Value::String("Устанавливаю сборку WCraft".into()));
        json_task(&["import-build", path, "--version", base, "--trust-unsigned"])
    });
    emit("progress", Value::Null);
    let _ = std::fs::remove_file(&file);
    let tag = result?["imported"].as_str().ok_or("Сборка WCraft не установилась")?.to_string();
    let kept = json!({ "sha256": client.sha256, "base": base, "tag": tag });
    std::fs::write(&record, kept.to_string()).map_err(|e| e.to_string())?;
    write_build_info(client, &tag)?;
    Ok(tag)
}

/// `native-mods.json` beside the imported program: the game makes room for its mods' own image (the voice chat
/// icons) and turns their render hooks on only when this file lists mods, and `import-build` copies the program alone.
fn write_build_info(client: &update::Release, tag: &str) -> Result<(), String> {
    if client.mods.is_empty() {
        return Ok(());
    }
    let file = home().join("versions").join(tag).join("native-mods.json");
    let info = json!({ "mods": client.mods }).to_string();
    if std::fs::read_to_string(&file).is_ok_and(|kept| kept == info) {
        return Ok(());
    }
    std::fs::write(&file, info).map_err(|e| format!("{}: {e}", file.display()))
}

/// Bytes the files under `dir` take.
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// What a first install of the game leaves in `home()`: the version with its prepared resources,
/// Mojang's Java and the server jar (1,47 GB measured for `nightly-2026-11-08-fdc87ae` on Linux).
const FIRST_INSTALL: u64 = 1_466_000_000;

/// Stops the `watch` thread when setup ends, however it ends, and hides the progress.
struct Watch(Arc<AtomicBool>, Emit);

impl Drop for Watch {
    fn drop(&mut self) {
        self.0.store(true, Relaxed);
        (self.1)("progress", Value::Null);
    }
}

/// While setup installs and prepares, sends `progress` about once a second: how many bytes of a
/// first install are on disk, of how many. The CLI prints no progress into a pipe, so the files
/// it writes are what tells how far it is; the window works out the time left from the pace.
/// What was there before (Minecraft's files when a new version replaces an old one) counts
/// neither as done nor as to do, so an update's bar starts empty.
fn watch(tag: &str, emit: &Emit) -> Watch {
    let stop = Arc::new(AtomicBool::new(false));
    let (flag, send) = (stop.clone(), emit.clone());
    let dirs = [home().join("versions").join(tag), home().join("minecraft"), home().join("server-jars")];
    let size = move || dirs.iter().map(|d| dir_size(d)).sum::<u64>();
    let start = size();
    // An update still writes the version itself, about half of a first install.
    let total = FIRST_INSTALL.saturating_sub(start).max(FIRST_INSTALL / 2);
    std::thread::spawn(move || loop {
        let done = size().saturating_sub(start);
        if flag.load(Relaxed) {
            break;
        }
        send("progress", json!({ "done": done, "total": total }));
        std::thread::sleep(Duration::from_secs(1));
    });
    Watch(stop, emit.clone())
}

const SERVER_NAME: &str = "WCraft";

/// The game's Multiplayer list: only our server the first time (Horizon's own picks go), then our entry kept
/// at the address the pack plays; servers the player adds stay.
fn keep_server_listed(instance: &str, server: &str) -> Result<(), String> {
    let marker = config::data_dir().join(format!("servers-listed-{instance}"));
    let first = !marker.exists();
    let list = json(&["multiplayer", "list", "--instance", instance])?;
    let entries = list.as_array().cloned().unwrap_or_default();
    let ours = |e: &Value| e["address"].as_str() == Some(server) && e["name"].as_str() == Some(SERVER_NAME);
    if !first && entries.iter().any(ours) {
        return Ok(());
    }
    // Removed by address, from the end, so the positions of the ones before don't move.
    for e in entries.iter().rev() {
        let stale_ours = e["name"].as_str() == Some(SERVER_NAME);
        if first || stale_ours {
            if let Some(address) = e["address"].as_str() {
                json(&["multiplayer", "remove", address, "--instance", instance])?;
            }
        }
    }
    json(&["multiplayer", "add", server, "--name", SERVER_NAME, "--instance", instance])?;
    std::fs::write(&marker, server).map_err(|e| e.to_string())
}

/// Starts the game on our server and returns when it exits.
pub fn play(slot: &Slot, emit: &Emit) -> Result<(), String> {
    let pack = config::pack();
    let settings = config::Settings::load();
    let server = settings.server();

    let Some(acc) = status().account else {
        return Err("Войдите в аккаунт WCraft".into());
    };
    let session = {
        // status() falls back to the first account when the CLI has none selected;
        // select it so the game does not join under its own made-up name.
        account_select(&acc.name)?;
        // The offline-mode server lets the name in only after this.
        emit("step", Value::String("Вхожу на сервер".into()));
        let session = crate::auth::join(&acc.name)?;
        // The skin this game shows its own player when the server gives none.
        if acc.offline {
            if let Some((skin_path, model)) = crate::skins::active_skin_file() {
                let _ = account_set_skin(&acc.name, &skin_path, &model);
            }
        }
        session
    };
    // Others see the skin the server gives with the profile, and this game reads theirs from its cache.
    emit("step", Value::String("Обновляю скины".into()));
    let log = |text: String| emit("log", Value::String(text));
    if let Err(e) = crate::skins::publish(&acc.name) {
        log(format!("Скин не опубликован: {e}"));
    }
    let game_dir = instance_home(&pack.instance).ok_or("Профиль WCraft не найден")?;
    if let Err(e) = crate::skins::sync_into(&game_dir) {
        log(format!("Скины игроков не обновлены: {e}"));
    }
    let _keeper = crate::skins::keep_synced(game_dir);
    if let Err(e) = keep_server_listed(&pack.instance, &server) {
        log(format!("Список серверов не обновлён: {e}"));
    }

    let mut args = vec!["play", "--instance", &pack.instance, "--server", &server];
    for arg in settings.game_args.split_whitespace() {
        args.extend(["--arg", arg]);
    }
    // The game joins with our session service's game token whenever the service gives one: an online-mode server
    // needs it, an offline-mode one ignores it. Not gated on `pack.sessions`, which a missed pack refresh leaves off.
    let env = match session {
        Some(session) => game_session_env(&session)?,
        None => Vec::new(),
    };
    run_with(&args, &env, slot, emit).map(drop)
}

/// The game's session file (read once, deleted by the game, at most 120 s old, owner-only) and the discovery
/// document naming our session service in place of Mojang's.
fn game_session_env(session: &Value) -> Result<Vec<(&'static str, String)>, String> {
    let file = config::data_dir().join("session.json");
    let text = json!({
        "name": session["name"], "uuid": session["uuid"], "access_token": session["token"], "user_type": "msa",
    });
    let _ = std::fs::remove_file(&file);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(&file).map_err(|e| e.to_string())?, text.to_string().as_bytes())
        .map_err(|e| format!("Файл сессии игры: {e}"))?;
    Ok(vec![
        ("HORIZON_SESSION_FILE", file.to_string_lossy().into_owned()),
        ("HORIZON_API_DISCOVERY", crate::auth::url("/session/discovery")),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pack moving the game to another version: setup installs it, moves the WCraft instance onto it and removes
    /// the old one. Against an accounts service with a pack published by scripts/release-pack.sh, in a data folder
    /// kept between runs (each run downloads a version, ~1.5 GB):
    /// `WCRAFT_HOME=<dir> WCRAFT_AUTH_URL=http://127.0.0.1:<port> cargo test -- --ignored --nocapture setup_follows`
    #[test]
    #[ignore]
    fn setup_follows_the_pack() {
        crate::update::refresh_pack().unwrap();
        let pack = config::pack();
        let before = installed_versions();
        let progress = Arc::new(Mutex::new(Vec::<u64>::new()));
        let seen = progress.clone();
        let emit: Emit = Arc::new(move |event, value| match event {
            "progress" => seen.lock().unwrap().extend(value["done"].as_u64()),
            _ => eprintln!("{event}: {value}"),
        });
        setup(&Mutex::new(None), &emit).unwrap();
        let progress = progress.lock().unwrap();
        eprintln!(
            "before {before:?}; progress done {:?} .. {:?} in {} reports",
            progress.first(),
            progress.last(),
            progress.len()
        );
        assert_eq!(installed_versions(), [pack.horizon_version.clone()]);
        assert!(prepared_dir(&pack.horizon_version).is_some());
        let instances = json(&["instance", "list"]).unwrap();
        eprintln!("instances: {instances}");
    }

    /// The pack's own game program: setup downloads it, imports it on the pack's version and moves the WCraft
    /// instance onto it; the next setup keeps it. The pack is taken unsigned from `WCRAFT_TEST_PACK` (a pack.json
    /// written by `scripts/release-pack.sh --client`), its zip from a service serving that folder:
    /// `WCRAFT_HOME=<dir> WCRAFT_AUTH_URL=http://127.0.0.1:<port> WCRAFT_TEST_PACK=<folder>/pack.json
    /// cargo test -- --ignored --nocapture setup_installs`
    #[test]
    #[ignore]
    fn setup_installs_the_client() {
        let file = std::env::var("WCRAFT_TEST_PACK").unwrap();
        let published: config::Published = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        config::save_published(&published).unwrap();
        let pack = config::pack();
        let client = pack.client.clone().expect("a client for this platform");
        let steps = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = steps.clone();
        let emit: Emit = Arc::new(move |event, value| {
            if event == "step" {
                seen.lock().unwrap().extend(value.as_str().map(String::from));
            }
            if event != "progress" {
                eprintln!("{event}: {value}");
            }
        });
        let downloads = || steps.lock().unwrap().iter().filter(|s| s.contains("Скачиваю сборку")).count();
        setup(&Mutex::new(None), &emit).unwrap();
        let kept: Value = serde_json::from_slice(&std::fs::read(config::data_dir().join("client.json")).unwrap()).unwrap();
        let tag = kept["tag"].as_str().unwrap().to_string();
        assert_eq!(kept["sha256"], client.sha256.as_str());
        let mut installed = installed_versions();
        installed.sort();
        assert_eq!(installed, [tag.clone(), pack.horizon_version.clone()]);
        assert!(prepared_dir(&tag).is_some(), "the imported version reuses the base's resources");
        let instances = json(&["instance", "list"]).unwrap();
        eprintln!("instances: {instances}");
        let downloaded = downloads();
        setup(&Mutex::new(None), &emit).unwrap();
        assert_eq!(downloads(), downloaded, "a second setup keeps the client");
    }
}

#[cfg(test)]
mod mirror_tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn archive_becomes_a_version_folder() {
        let root = std::env::temp_dir().join(format!("wcraft-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let top = root.join("src/horizon-linux-x86_64");
        std::fs::create_dir_all(&top).unwrap();
        std::fs::write(top.join("horizon-client"), "bin").unwrap();
        let archive = root.join("a.tar.gz");
        let gz = flate2::write::GzEncoder::new(std::fs::File::create(&archive).unwrap(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        tar.append_dir_all("horizon-linux-x86_64", &top).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        std::fs::create_dir_all(root.join("versions")).unwrap();
        let target = root.join("versions/v1");
        place_version(&archive, &root.join("unpacked"), &target).unwrap();
        assert!(target.join("horizon-client").is_file() && target.join(".installed").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The real archive through `place_version`, then the CLI must count it installed:
    /// `WCRAFT_HOME=<empty dir> WCRAFT_TEST_BASE=<horizon-linux-x86_64.tar.gz> cargo test -- --ignored mirror_install`
    #[cfg(not(windows))]
    #[test]
    #[ignore]
    fn mirror_install_is_seen_by_the_cli() {
        let archive = PathBuf::from(std::env::var_os("WCRAFT_TEST_BASE").expect("WCRAFT_TEST_BASE"));
        let tag = config::pack().horizon_version;
        let versions = home().join("versions");
        std::fs::create_dir_all(&versions).unwrap();
        place_version(&archive, &versions.join(".x.partial/unpacked"), &versions.join(&tag)).unwrap();
        assert!(is_installed(&tag));
    }
}
