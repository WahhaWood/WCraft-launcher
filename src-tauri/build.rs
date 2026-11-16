use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rustc-env=TARGET={}", env::var("TARGET").unwrap_or_default());
    embed_cli();
    tauri_build::build()
}

/// `WCRAFT_EMBED_CLI=<path to horizon-launcher>` bakes the CLI into the executable (gzip),
/// so the Windows download is a single `.exe`. Unset: nothing is embedded (dev builds).
fn embed_cli() {
    println!("cargo::rustc-check-cfg=cfg(embedded_cli)");
    println!("cargo:rerun-if-env-changed=WCRAFT_EMBED_CLI");
    let Some(src) = env::var_os("WCRAFT_EMBED_CLI").map(PathBuf::from) else { return };
    println!("cargo:rerun-if-changed={}", src.display());

    let raw = fs::read(&src).unwrap_or_else(|e| panic!("WCRAFT_EMBED_CLI {}: {e}", src.display()));
    let hash = Sha256::digest(&raw).iter().map(|b| format!("{b:02x}")).collect::<String>();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("cli.gz");
    let mut gz = flate2::write::GzEncoder::new(fs::File::create(&out).unwrap(), flate2::Compression::best());
    std::io::Write::write_all(&mut gz, &raw).unwrap();
    gz.finish().unwrap();

    println!("cargo:rustc-env=WCRAFT_CLI_HASH={}", &hash[..12]);
    println!("cargo:rustc-cfg=embedded_cli");
}
