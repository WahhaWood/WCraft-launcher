//! Self-update. The accounts service serves the launcher's releases (`scripts/release-launcher.sh`):
//! `/launcher/latest.json` names the newest version and its file per platform with SHA-256, and
//! `/launcher/latest.json.sig` is the manifest's Ed25519 signature by the WCraft release key built in here.
//! A newer release replaces the file the launcher runs from: the .exe on Windows (the running one is
//! renamed to `.old` and removed on the next start), the AppImage on Linux. Dev runs never update.
//! The same folder holds `pack.json` (`scripts/release-pack.sh`): the game version and server to play, signed
//! the same way, so the game moves to a new version without a new launcher.

use crate::{applog, auth, config, horizon};
use base64::Engine;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The WCraft release key (horizon-cli/crates/horizon-release-sign/release-keys.txt).
const KEY: &str = "/zyCdZeMkjIGoA/wHB2QWJ2MRoJtgCbBARgTLXoM6JM=";
const KIND: &str = "wcraft-launcher";
const PACK_KIND: &str = "wcraft-pack";
pub const PLATFORM: &str = if cfg!(windows) { "windows-x86_64" } else { "linux-x86_64" };
/// No build comes near this; a bigger download is not ours.
const MAX_SIZE: u64 = 512 << 20;

#[derive(Deserialize)]
struct Kind {
    kind: String,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    files: BTreeMap<String, Release>,
}

/// A file in the releases folder: a launcher build, or the pack's game program (`pack.json` `client`).
#[derive(Deserialize, Serialize, Clone)]
pub struct Release {
    pub name: String,
    pub sha256: String,
    pub size: u64,
    /// The mods compiled into a game program (`native-mods.json` `mods` of its build); none for a launcher build.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<serde_json::Value>,
}

#[derive(Serialize, Clone)]
pub struct Update {
    pub version: String,
    pub size: u64,
}

/// The file the launcher runs from, which an update replaces; none in dev runs and unpackaged builds.
pub fn target() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return None;
    }
    if cfg!(windows) {
        std::env::current_exe().ok()
    } else {
        std::env::var_os("APPIMAGE").map(PathBuf::from)
    }
}

/// Removes what the last update left: the replaced .exe, a download cut short.
pub fn clean_up() {
    if let Some(target) = target() {
        let _ = fs::remove_file(sibling(&target, "old"));
        let _ = fs::remove_file(sibling(&target, "new"));
    }
}

fn sibling(target: &Path, ext: &str) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{ext}"));
    target.with_file_name(name)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .build()
}

fn get(agent: &ureq::Agent, path: &str) -> Result<ureq::Response, String> {
    let mut tries = 0;
    loop {
        match agent.get(&auth::url(path)).call() {
            Ok(response) => return Ok(response),
            Err(ureq::Error::Status(code, _)) => return Err(format!("Сервер обновлений ответил {code}")),
            // Not connected: look for another address of the service (a mirror) and try once more.
            Err(_) if tries == 0 => {
                tries += 1;
                auth::failed();
            }
            Err(_) => return Err("Сервер обновлений недоступен".into()),
        }
    }
}

/// `path` and its `.sig` from the service, checked to be a `kind` manifest signed by the WCraft key.
fn fetch_signed<T: DeserializeOwned>(agent: &ureq::Agent, path: &str, kind: &str) -> Result<T, String> {
    let mut json = Vec::new();
    get(agent, path)?.into_reader().take(1 << 16).read_to_end(&mut json).map_err(|e| e.to_string())?;
    let sig = get(agent, &format!("{path}.sig"))?.into_string().map_err(|e| e.to_string())?;
    verify(&json, &sig, KEY, kind)
}

/// Takes the pack the service publishes when it is newer than the one kept; true when it changed.
pub fn refresh_pack() -> Result<bool, String> {
    let pack: config::Published = fetch_signed(&agent(), "/launcher/pack.json", PACK_KIND)?;
    if config::published().is_some_and(|kept| kept.serial >= pack.serial) {
        return Ok(false);
    }
    config::save_published(&pack)?;
    Ok(true)
}

/// The newest release for this platform, if it is newer than this launcher.
fn newest(agent: &ureq::Agent) -> Result<Option<(String, Release)>, String> {
    let manifest: Manifest = fetch_signed(agent, "/launcher/latest.json", KIND)?;
    if !newer(&manifest.version, env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }
    Ok(manifest.files.get(PLATFORM).map(|r| (manifest.version.clone(), r.clone())))
}

/// Asks the server for a newer launcher; none when this one cannot update itself.
pub fn check() -> Result<Option<Update>, String> {
    if target().is_none() {
        return Ok(None);
    }
    Ok(newest(&agent())?.map(|(version, r)| Update { version, size: r.size }))
}

/// Downloads the newest release, checks it and puts it in place of the running launcher; returns the file
/// to start. `progress(done, total)` follows the download.
pub fn install(progress: impl Fn(u64, u64)) -> Result<PathBuf, String> {
    let target = target().ok_or("Эта сборка не обновляется сама")?;
    let agent = agent();
    let (_, release) = newest(&agent)?.ok_or("Обновлений нет")?;
    if release.size > MAX_SIZE {
        return Err("Обновление слишком большое".into());
    }
    let new = sibling(&target, "new");
    let result = download_client(&release, &new, &progress).and_then(|()| replace(&target, &new));
    if result.is_err() {
        let _ = fs::remove_file(&new);
    }
    result.map(|()| target)
}

/// Downloads the pack's game program to `to`, checked against its size and SHA-256.
pub fn download_client(client: &Release, to: &Path, progress: &impl Fn(u64, u64)) -> Result<(), String> {
    if client.size > MAX_SIZE {
        return Err("Сборка игры слишком большая".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(6))
        .timeout_read(Duration::from_millis(750))
        .build();
    let result = horizon::interruptible(auth::download_endpoints)
        .and_then(|endpoints| download_from_mirrors(&agent, endpoints, client, to, progress));
    if result.is_err() {
        let _ = fs::remove_file(to);
    }
    result
}

fn download_from_mirrors(
    agent: &ureq::Agent,
    mut endpoints: Vec<String>,
    release: &Release,
    to: &Path,
    progress: &impl Fn(u64, u64),
) -> Result<(), String> {
    let path = format!("/launcher/{}", release.name);
    let mut error = "Сервер обновлений недоступен".to_owned();
    while !endpoints.is_empty() {
        let index = if endpoints.len() == 1 || release.size == 0 {
            0
        } else {
            let (mirrors, path) = (endpoints.clone(), path.clone());
            let sample = release.size.min(256 * 1024);
            horizon::interruptible(move || fastest_mirror(mirrors, &path, sample))?.unwrap_or(0)
        };
        let url = format!("{}{}", endpoints.remove(index), path);
        progress(0, release.size);
        match download(agent, url, release, to, progress) {
            Ok(()) => return Ok(()),
            Err(reason) => {
                horizon::check_cancel()?;
                applog::line(&format!("download {} failed: {reason}", release.name));
                error = reason;
            }
        }
    }
    Err(error)
}

fn fastest_mirror(endpoints: Vec<String>, path: &str, sample: u64) -> Option<usize> {
    let timeout = Duration::from_secs(3);
    let (tx, rx) = std::sync::mpsc::channel();
    for (index, endpoint) in endpoints.into_iter().enumerate() {
        let (tx, url) = (tx.clone(), format!("{endpoint}{path}"));
        std::thread::spawn(move || {
            let request = ureq::get(&url).timeout(timeout)
                .set("Range", &format!("bytes=0-{}", sample - 1));
            if let Ok(response) = request.call() {
                let mut prefix = response.into_reader().take(sample);
                if std::io::copy(&mut prefix, &mut std::io::sink()).ok() == Some(sample) {
                    let _ = tx.send(index);
                }
            }
        });
    }
    drop(tx);
    rx.recv_timeout(timeout).ok()
}

fn download(agent: &ureq::Agent, url: String, release: &Release, to: &Path, progress: &impl Fn(u64, u64)) -> Result<(), String> {
    let agent = agent.clone();
    let response = horizon::interruptible(move || {
        agent.get(&url).call().map_err(|error| match error {
            ureq::Error::Status(code, _) => format!("Сервер загрузок ответил {code}"),
            ureq::Error::Transport(_) => "Сервер загрузок недоступен".into(),
        })
    })??;
    let mut reader = response.into_reader().take(release.size + 1);
    let mut file = fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let (mut hash, mut done, mut buf) = (Sha256::new(), 0u64, vec![0u8; 1 << 16]);
    let mut last_data = std::time::Instant::now();
    loop {
        let n = horizon::read_chunk(&mut reader, &mut buf, &mut last_data, Duration::from_secs(6))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", to.display()))?;
        done += n as u64;
        progress(done, release.size);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    let hex: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if done != release.size || hex != release.sha256 {
        return Err("Обновление скачалось с ошибкой, попробуйте ещё раз".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(to, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Puts `new` in place of `target`. Windows won't overwrite a running .exe but lets it be renamed.
fn replace(target: &Path, new: &Path) -> Result<(), String> {
    let failed = |e: std::io::Error| format!("Не удалось заменить {}: {e}", target.display());
    if cfg!(windows) {
        let old = sibling(target, "old");
        let _ = fs::remove_file(&old);
        fs::rename(target, &old).map_err(failed)?;
        if let Err(e) = fs::rename(new, target) {
            let _ = fs::rename(&old, target);
            return Err(failed(e));
        }
        Ok(())
    } else {
        fs::rename(new, target).map_err(failed)
    }
}

/// The manifest, if `sig` (base64) is `key`'s signature of exactly these bytes and its `kind` is this one.
fn verify<T: DeserializeOwned>(json: &[u8], sig: &str, key: &str, kind: &str) -> Result<T, String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let bad = || "Подпись обновления не сходится".to_string();
    let key: [u8; 32] = b64.decode(key).ok().and_then(|k| k.try_into().ok()).ok_or_else(bad)?;
    let sig: [u8; 64] = b64.decode(sig.trim()).ok().and_then(|s| s.try_into().ok()).ok_or_else(bad)?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| bad())?;
    key.verify_strict(json, &ed25519_dalek::Signature::from_bytes(&sig)).map_err(|_| bad())?;
    let signed: Kind = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    if signed.kind != kind {
        return Err(bad());
    }
    serde_json::from_slice(json).map_err(|e| e.to_string())
}

/// `a` > `b` as dotted numbers (`0.1.10` > `0.1.9`).
fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_a_good_mirror_after_the_fastest_sends_an_incomplete_file() {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;

        let archive = vec![42; 300 * 1024];
        let release = Release {
            name: "mirror-test.zip".into(),
            sha256: format!("{:x}", Sha256::digest(&archive)),
            size: archive.len() as u64,
            mods: Vec::new(),
        };
        let mirror = |body: Vec<u8>, probe_delay| {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let worker = std::thread::spawn(move || {
                for _ in 0..2 {
                    let (mut socket, _) = listener.accept().unwrap();
                    socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut reader = BufReader::new(&socket);
                    let mut probe = false;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                            break;
                        }
                        probe |= line.to_ascii_lowercase().starts_with("range:");
                    }
                    if probe {
                        std::thread::sleep(probe_delay);
                    }
                    write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                    let _ = socket.write_all(&body);
                }
            });
            (endpoint, worker)
        };
        let (good, good_server) = mirror(archive.clone(), Duration::from_millis(200));
        let (fast, fast_server) = mirror(archive[..256 * 1024].to_vec(), Duration::ZERO);
        let file = std::env::temp_dir().join(format!("wcraft-mirror-test-{}.zip", std::process::id()));
        let attempts = std::cell::Cell::new(0);
        let result = download_from_mirrors(
            &agent(), vec![good, fast], &release, &file, &|done, _| {
                if done == 0 {
                    attempts.set(attempts.get() + 1);
                }
            },
        );
        let downloaded = fs::read(&file);
        let _ = fs::remove_file(&file);
        result.unwrap();
        assert_eq!(attempts.get(), 2);
        assert_eq!(downloaded.unwrap(), archive);
        good_server.join().unwrap();
        fast_server.join().unwrap();
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("0.2.0", "0.1.9"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn only_a_signed_launcher_manifest_passes() {
        // Signed by scripts/release-launcher.sh's openssl call with a throwaway key.
        let dir = std::env::temp_dir().join(format!("wcraft-update-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let key = dir.join("key.pem");
        let ok = |out: std::process::Output| assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let openssl = |args: &[&str]| std::process::Command::new("openssl").args(args).output().unwrap();
        ok(openssl(&["genpkey", "-algorithm", "ed25519", "-out", key.to_str().unwrap()]));
        let pubkey = openssl(&["pkey", "-in", key.to_str().unwrap(), "-pubout", "-outform", "DER"]).stdout;
        let b64 = base64::engine::general_purpose::STANDARD;
        let pubkey = b64.encode(&pubkey[pubkey.len() - 32..]);
        let sign = |json: &str| {
            let file = dir.join("latest.json");
            fs::write(&file, json).unwrap();
            let out = openssl(&["pkeyutl", "-sign", "-inkey", key.to_str().unwrap(), "-rawin", "-in", file.to_str().unwrap()]);
            b64.encode(out.stdout)
        };

        let json = r#"{"kind":"wcraft-launcher","version":"9.0.0","files":{"linux-x86_64":{"name":"a","sha256":"b","size":1}}}"#;
        let sig = sign(json);
        let check = |json: &str, sig: &str, key: &str| verify::<Manifest>(json.as_bytes(), sig, key, KIND);
        assert_eq!(check(json, &sig, &pubkey).unwrap().version, "9.0.0");
        let changed = json.replace("9.0.0", "9.0.1");
        assert!(check(&changed, &sig, &pubkey).is_err());
        assert!(check(json, &sig, KEY).is_err());
        let other = r#"{"kind":"horizon","version":"9.0.0","files":{}}"#;
        assert!(check(other, &sign(other), &pubkey).is_err());
        // A pack is signed the same way but never passes for a launcher, nor the other way round.
        let pack = r#"{"kind":"wcraft-pack","serial":1,"horizon_version":"v1","server":"a:1"}"#;
        let pack_sig = sign(pack);
        let published: config::Published = verify(pack.as_bytes(), &pack_sig, &pubkey, PACK_KIND).unwrap();
        assert_eq!((published.serial, published.horizon_version.as_str()), (1, "v1"));
        assert!(check(pack, &pack_sig, &pubkey).is_err());
        assert!(verify::<config::Published>(json.as_bytes(), &sig, &pubkey, PACK_KIND).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Against an accounts service with a newer release signed by scripts/release-launcher.sh (release build:
    /// debug builds never update): `APPIMAGE=<a copy> WCRAFT_AUTH_URL=http://127.0.0.1:<port>
    /// cargo test --release -- --ignored update_installs`
    #[test]
    #[ignore]
    #[cfg(not(windows))]
    fn update_installs_from_the_service() {
        let update = check().unwrap().expect("the service has a newer release");
        let target = install(|_, _| {}).unwrap();
        assert_eq!(target, target_path());
        assert_eq!(fs::metadata(&target).unwrap().len(), update.size);
        assert!(!sibling(&target, "new").exists());
    }

    /// Against an accounts service with a pack published by scripts/release-pack.sh, in an empty data folder:
    /// `WCRAFT_HOME=<empty dir> WCRAFT_AUTH_URL=http://127.0.0.1:<port> cargo test -- --ignored pack_refreshes`
    #[test]
    #[ignore]
    fn pack_refreshes_from_the_service() {
        assert!(config::published().is_none(), "WCRAFT_HOME must start empty");
        assert!(refresh_pack().unwrap());
        let published = config::published().unwrap();
        let pack = config::pack();
        assert_eq!((&pack.horizon_version, &pack.server), (&published.horizon_version, &published.server));
        assert!(!refresh_pack().unwrap(), "the same pack again is no change");
        config::save_published(&config::Published { serial: published.serial + 1, ..published }).unwrap();
        assert!(!refresh_pack().unwrap(), "an older pack is not taken");
    }

    #[cfg(not(windows))]
    fn target_path() -> PathBuf {
        PathBuf::from(std::env::var_os("APPIMAGE").unwrap())
    }

    #[test]
    fn replace_swaps_the_file() {
        let dir = std::env::temp_dir().join(format!("wcraft-replace-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (target, new) = (dir.join("WCraft.exe"), dir.join("WCraft.exe.new"));
        fs::write(&target, "old").unwrap();
        fs::write(&new, "new").unwrap();
        replace(&target, &new).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        assert!(!new.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
