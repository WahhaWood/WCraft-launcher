//! `launcher.log` in the data folder: what the launcher did and what failed, for a player to send when something
//! goes wrong (the window shows only the last step). One older log is kept as `launcher.log.old`.

use std::io::Write;

const MAX: u64 = 256 << 10;

fn file() -> std::path::PathBuf {
    crate::config::data_dir().join("launcher.log")
}

/// UTC `YYYY-MM-DD HH:MM:SS` of now.
fn stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = ((secs / 86400) as i64, secs % 86400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3600, rest % 3600 / 60, rest % 60)
}

pub fn line(text: &str) {
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file()) else { return };
    let _ = writeln!(f, "{} {}", stamp(), text.replace('\n', " | "));
}

/// At launcher start: rotates a big log and notes the version and the pack the launcher holds.
pub fn start() {
    let path = file();
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    line(&format!("launcher {} {}", env!("CARGO_PKG_VERSION"), pack_summary()));
}

/// The kept pack in one line: its serial and the game program it names for this platform.
pub fn pack_summary() -> String {
    match crate::config::published() {
        Some(p) => {
            let client = p.client.get(crate::update::PLATFORM).map_or("no client".to_string(), |c| {
                format!("client {} {} ({} mods)", c.name, &c.sha256[..c.sha256.len().min(12)], c.mods.len())
            });
            format!("pack serial {} {} {}", p.serial, p.horizon_version, client)
        }
        None => "no pack kept".to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_stamp_reads_as_a_date_and_time() {
        let s = super::stamp();
        let b = s.as_bytes();
        assert_eq!(s.len(), 19, "{s}");
        assert!(b[4] == b'-' && b[7] == b'-' && b[10] == b' ' && b[13] == b':' && b[16] == b':', "{s}");
        assert!(s.starts_with("202"), "{s}");
    }
}
