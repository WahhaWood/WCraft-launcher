//! Minecraft Server List Ping: who is online, without joining.
use serde::Serialize;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[derive(Serialize)]
pub struct ServerStatus {
    pub online: u32,
    pub max: u32,
    pub version: String,
    pub ping_ms: u32,
    pub players: Vec<String>,
}

fn varint(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn read_varint(r: &mut impl Read) -> std::io::Result<u32> {
    let mut v = 0u32;
    for i in 0..5 {
        let mut b = [0u8];
        r.read_exact(&mut b)?;
        v |= ((b[0] & 0x7f) as u32) << (7 * i);
        if b[0] & 0x80 == 0 {
            break;
        }
    }
    Ok(v)
}

fn packet(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    varint(&mut out, body.len() as u32);
    out.extend_from_slice(body);
    out
}

pub fn ping(address: &str) -> Result<ServerStatus, String> {
    let (host, port) = match address.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| "Неверный порт")?),
        None => (address, 25565),
    };
    let timeout = Duration::from_secs(3);
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("Адрес не найден")?;
    let start = Instant::now();
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(timeout)).ok();

    // Handshake (next state: status), then the status request.
    let mut hs = vec![0x00];
    // A real protocol number (26.3): playit.gg's Minecraft tunnel resets pings that send -1.
    varint(&mut hs, 777);
    varint(&mut hs, host.len() as u32);
    hs.extend_from_slice(host.as_bytes());
    hs.extend_from_slice(&port.to_be_bytes());
    hs.push(1);
    let mut out = packet(&hs);
    out.extend(packet(&[0x00]));
    s.write_all(&out).map_err(|e| e.to_string())?;

    let io = |e: std::io::Error| e.to_string();
    let _len = read_varint(&mut s).map_err(io)?;
    let _id = read_varint(&mut s).map_err(io)?;
    let n = read_varint(&mut s).map_err(io)? as usize;
    if n > 1 << 20 {
        return Err("Слишком большой ответ".into());
    }
    let mut json = vec![0u8; n];
    s.read_exact(&mut json).map_err(io)?;
    let ping_ms = start.elapsed().as_millis() as u32;

    let v: serde_json::Value = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    let num = |k: &str| v["players"][k].as_u64().unwrap_or(0) as u32;
    Ok(ServerStatus {
        online: num("online"),
        max: num("max"),
        version: v["version"]["name"].as_str().unwrap_or("").to_string(),
        ping_ms,
        players: v["players"]["sample"]
            .as_array()
            .map(|a| a.iter().filter_map(|p| p["name"].as_str().map(String::from)).collect())
            .unwrap_or_default(),
    })
}
