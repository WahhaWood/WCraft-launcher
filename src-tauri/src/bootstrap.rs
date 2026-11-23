//! Downloads the `horizon-launcher` CLI from Horizon-Offline-Launcher releases when the player has none.
use crate::horizon::{self, Emit};
use serde_json::Value;
use sha2::{Digest, Sha256};

const RELEASES: &str = "https://api.github.com/repos/gerdaroot/Horizon-Offline-Launcher/releases?per_page=30";
const ASSET: &str = if cfg!(windows) {
    "horizon-launcher-windows-x64.zip"
} else {
    "horizon-launcher-linux-x86_64.tar.gz"
};

const FALLBACK_URL: &str = if cfg!(windows) {
    "https://github.com/gerdaroot/Horizon-Offline-Launcher/releases/download/launcher%401.4.0/horizon-launcher-windows-x64.zip"
} else {
    "https://github.com/gerdaroot/Horizon-Offline-Launcher/releases/download/launcher%401.4.0/horizon-launcher-linux-x86_64.tar.gz"
};

fn get(url: &str) -> Result<ureq::Response, String> {
    let url = url.to_string();
    horizon::interruptible(move || {
        ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout_read(std::time::Duration::from_millis(250))
            .build()
            .get(&url)
            .set("User-Agent", "wcraft-launcher")
            .call()
            .map_err(|e| e.to_string())
    })?
}

pub fn install(emit: &Emit) -> Result<(), String> {
    if horizon::cli().is_some() {
        return Ok(());
    }
    let log = |text: String| emit("log", Value::String(text));

    let (url, expected_digest, expected_size) = match get(RELEASES).and_then(|r| r.into_json::<Value>().map_err(|e| e.to_string())) {
        Ok(releases) => {
            let asset = releases
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|r| r["assets"].as_array().into_iter().flatten())
                .find(|a| a["name"] == ASSET);

            if let Some(asset) = asset {
                let u = asset["browser_download_url"].as_str().map(str::to_string);
                let d = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string);
                let s = asset["size"].as_u64();
                (u.unwrap_or_else(|| FALLBACK_URL.to_string()), d, s)
            } else {
                (FALLBACK_URL.to_string(), None, None)
            }
        }
        Err(_) => (FALLBACK_URL.to_string(), None, None),
    };

    let size = expected_size.unwrap_or(28 * 1024 * 1024).max(1);
    log(format!("Скачиваю {ASSET}"));

    let mut reader = get(&url)?.into_reader();
    let mut bytes = Vec::with_capacity(size as usize);
    let mut buf = vec![0u8; 1 << 16];
    let mut shown = 0;
    let mut last_data = std::time::Instant::now();
    loop {
        let n = horizon::read_chunk(&mut reader, &mut buf, &mut last_data, std::time::Duration::from_secs(30))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        let percent = bytes.len() as u64 * 100 / size;
        if percent >= shown + 10 {
            shown = percent;
            log(format!("Скачано {percent}%"));
        }
    }
    // GitHub publishes each asset's SHA-256.
    if let Some(want) = expected_digest {
        let got: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if got != want {
            return Err("Загрузка повреждена: контрольная сумма не совпала".into());
        }
    }

    let dir = horizon::bin_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .and_then(|mut z| z.extract(&dir))
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    tar::Archive::new(flate2::read::GzDecoder::new(&bytes[..])).unpack(&dir).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(exe) = horizon::find_in(&dir, 2) {
            let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
        }
    }

    horizon::cli().map(|_| ()).ok_or_else(|| "В архиве нет horizon-launcher".to_string())
}
