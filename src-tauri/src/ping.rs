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
    pub ping_ms: Option<u32>,
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
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(timeout)).ok();
    s.set_nodelay(true).ok();

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
    let v: serde_json::Value = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    let ping_ms = measure_rtt(&mut s);
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

fn measure_rtt(socket: &mut TcpStream) -> Option<u32> {
    let mut ping = [0; 9];
    ping[0] = 1;
    ping[1..].copy_from_slice(&1u64.to_be_bytes());
    let request = packet(&ping);
    let start = Instant::now();
    socket.write_all(&request).ok()?;
    let length = read_varint(socket).ok()?;
    if length != ping.len() as u32 {
        return None;
    }
    let mut pong = [0; 9];
    socket.read_exact(&mut pong).ok()?;
    (pong == ping).then(|| start.elapsed().as_millis() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn excludes_status_response_delay_from_ping() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let delay = Duration::from_millis(250);
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let receive = |socket: &mut TcpStream| {
                let mut bytes = vec![0; read_varint(socket).unwrap() as usize];
                socket.read_exact(&mut bytes).unwrap();
                bytes
            };
            receive(&mut socket);
            assert_eq!(receive(&mut socket), [0]);
            std::thread::sleep(delay);
            let json = br#"{"players":{"online":1,"max":20}}"#;
            let mut status = vec![0];
            varint(&mut status, json.len() as u32);
            status.extend_from_slice(json);
            socket.write_all(&packet(&status)).unwrap();
            let mut request = [0; 10];
            if socket.read_exact(&mut request).is_ok() {
                assert_eq!(&request[..2], &[9, 1]);
                socket.write_all(&request).unwrap();
            }
        });
        let status = ping(&address).unwrap();
        server.join().unwrap();
        assert_eq!(status.online, 1);
        assert!((status.ping_ms.unwrap() as u128) < delay.as_millis());
    }
}
