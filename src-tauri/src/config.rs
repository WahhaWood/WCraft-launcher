use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

/// What the launcher plays: baked in (`wcraft.json`), the game version and server overridden by the pack the
/// service published last.
#[derive(Deserialize, Serialize, Clone)]
pub struct Pack {
    pub server: String,
    pub horizon_version: String,
    pub instance: String,
    /// The accounts service (backend/auth): registration, login, join. Its address when `endpoint` can't say.
    pub auth: String,
    /// Where the accounts service's current address is published (scripts/publish-endpoint.sh): its tunnel
    /// gets a new one on every start.
    #[serde(default)]
    pub endpoint: String,
    /// The game program with the pack's mods compiled in, for this platform (`Published::client`).
    #[serde(default)]
    pub client: Option<crate::update::Release>,
    /// The server runs in online mode with our session service (`Published::sessions`).
    #[serde(default)]
    pub sessions: bool,
    /// The sources of the pack's client mods (`Published::mods_source`).
    #[serde(default)]
    pub mods_source: Option<crate::update::Release>,
}

pub fn pack() -> Pack {
    let mut pack: Pack = serde_json::from_str(include_str!("../wcraft.json")).expect("wcraft.json");
    if let Some(published) = published() {
        pack.horizon_version = published.horizon_version;
        pack.server = published.server;
        pack.client = published.client.get(crate::update::PLATFORM).cloned();
        pack.sessions = published.sessions;
        pack.mods_source = published.mods_source;
    }
    pack
}

/// The pack the service publishes at `/launcher/pack.json` (`scripts/release-pack.sh`), kept here once its
/// signature checked (`update::refresh_pack`). `serial` only grows: an older pack, even signed, is not taken.
#[derive(Deserialize, Serialize)]
pub struct Published {
    pub serial: u64,
    pub horizon_version: String,
    pub server: String,
    /// Per platform (`update::PLATFORM`): the game program we build with the pack's mods, a zip in the releases
    /// folder (`scripts/release-pack.sh --client`). None: the version as Horizon published it.
    #[serde(default)]
    pub client: BTreeMap<String, crate::update::Release>,
    /// The server runs in online mode: the game gets a session from our service (`release-pack.sh --sessions`).
    #[serde(default)]
    pub sessions: bool,
    /// A zip of the client mods' sources (`mods/client`, `release-pack.sh --mods-source`): the experimental mods page
    /// compiles them with the player's own mods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mods_source: Option<crate::update::Release>,
}

pub fn published() -> Option<Published> {
    fs::read(data_dir().join("pack.json")).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn save_published(pack: &Published) -> Result<(), String> {
    let (file, new) = (data_dir().join("pack.json"), data_dir().join("pack.json.new"));
    let json = serde_json::to_vec_pretty(pack).map_err(|e| e.to_string())?;
    fs::write(&new, json).and_then(|()| fs::rename(&new, &file)).map_err(|e| e.to_string())
}

pub fn data_dir() -> PathBuf {
    let dir = std::env::var_os("WCRAFT_HOME").map(PathBuf::from).unwrap_or_else(|| {
        dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("wcraft-launcher")
    });
    let _ = fs::create_dir_all(&dir);
    dir
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// Empty: the pack's server.
    pub server: String,
    /// stay | minimize | hide
    pub while_playing: String,
    pub game_args: String,
    pub accent: String,
    /// Experimental: the Mods page, own native mods built into the game with WCraft's (`mods.rs`).
    pub experimental_mods: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: String::new(),
            while_playing: "minimize".into(),
            game_args: String::new(),
            accent: "violet".into(),
            experimental_mods: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        fs::read(data_dir().join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(data_dir().join("settings.json"), json).map_err(|e| e.to_string())
    }

    pub fn server(&self) -> String {
        let s = self.server.trim();
        if s.is_empty() { pack().server } else { s.to_string() }
    }
}
