//! Steam's `userdata/<id>/config/shortcuts.vdf` (binary KeyValues): the user's non-Steam games.
use crate::config::home;
use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
pub struct Shortcut {
    pub appid: u32,
    pub name: String,
    pub exe: String,
    pub start_dir: String,
    pub launch_options: String,
}

fn cstr(b: &[u8], i: &mut usize) -> String {
    let start = *i;
    while *i < b.len() && b[*i] != 0 { *i += 1; }
    let s = String::from_utf8_lossy(&b[start..*i]).to_string();
    *i += 1;
    s
}

/// Parse one KeyValues object starting after its name; fills `out` with the shortcut entries it finds.
fn object(b: &[u8], i: &mut usize, depth: usize, out: &mut Vec<Shortcut>) {
    let mut cur = Shortcut::default();
    let mut has = false;
    while *i < b.len() {
        let t = b[*i];
        *i += 1;
        if t == 0x08 { break; }
        let key = cstr(b, i).to_lowercase();
        match t {
            0x00 => object(b, i, depth + 1, out),
            0x01 => {
                let v = cstr(b, i);
                has = true;
                match key.as_str() {
                    "appname" => cur.name = v,
                    "exe" => cur.exe = v.trim_matches('"').to_string(),
                    "startdir" => cur.start_dir = v.trim_matches('"').to_string(),
                    "launchoptions" => cur.launch_options = v,
                    _ => {}
                }
            }
            0x02 => {
                if *i + 4 <= b.len() {
                    let v = u32::from_le_bytes([b[*i], b[*i + 1], b[*i + 2], b[*i + 3]]);
                    if key == "appid" { cur.appid = v; has = true; }
                }
                *i += 4;
            }
            0x07 => *i += 8,
            _ => return,
        }
    }
    if has && depth == 2 && !cur.name.is_empty() { out.push(cur); }
}

pub fn list() -> Vec<Shortcut> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(home().join(".local/share/Steam/userdata")) else { return out };
    for u in rd.flatten() {
        let Ok(b) = std::fs::read(u.path().join("config/shortcuts.vdf")) else { continue };
        if b.first() == Some(&0x00) {
            let mut i = 1;
            let _ = cstr(&b, &mut i);
            object(&b, &mut i, 1, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        let mut b = vec![0u8];
        b.extend(b"shortcuts\0");
        b.push(0); b.extend(b"0\0");
        b.push(2); b.extend(b"appid\0"); b.extend(2602417635u32.to_le_bytes());
        b.push(1); b.extend(b"AppName\0Tunic\0");
        b.push(1); b.extend(b"Exe\0\"/games/Tunic.exe\"\0");
        b.push(1); b.extend(b"StartDir\0/games/\0");
        b.push(0); b.extend(b"tags\0"); b.push(8);
        b.push(8); b.push(8); b.push(8);
        let mut i = 1; let _ = cstr(&b, &mut i); let mut out = vec![];
        object(&b, &mut i, 1, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].appid, 2602417635);
        assert_eq!(out[0].exe, "/games/Tunic.exe");
    }
}
