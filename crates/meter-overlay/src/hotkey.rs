use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::{Modifiers, Shortcut};

use crate::persist;

pub const DEFAULT: &str = "Ctrl+Shift+KeyL";
const FILE_NAME: &str = "overlay.json";

#[derive(Default, Serialize, Deserialize)]
struct Settings {
    lock_shortcut: Option<String>,
}

pub fn parse(accel: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(accel).map_err(|e| format!("invalid shortcut: {e}"))?;
    let any_modifier = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT | Modifiers::SUPER;
    if !shortcut.mods.intersects(any_modifier) {
        return Err("use Ctrl, Alt, Shift or Win".into());
    }
    Ok(shortcut)
}

fn display_part(part: &str) -> &str {
    match part {
        "Control" | "CONTROL" | "ctrl" => "Ctrl",
        "Super" | "SUPER" | "Meta" => "Win",
        _ => part
            .strip_prefix("Key")
            .or_else(|| part.strip_prefix("Digit"))
            .filter(|key| !key.is_empty())
            .unwrap_or(part),
    }
}

pub fn display(accel: &str) -> String {
    accel
        .split('+')
        .map(|part| display_part(part.trim()))
        .collect::<Vec<_>>()
        .join("+")
}

pub fn load(dir: &Path) -> String {
    fs::read_to_string(dir.join(FILE_NAME))
        .ok()
        .and_then(|json| serde_json::from_str::<Settings>(&json).ok())
        .and_then(|settings| settings.lock_shortcut)
        .filter(|accel| parse(accel).is_ok())
        .unwrap_or_else(|| DEFAULT.to_string())
}

pub fn save(dir: &Path, accel: &str) -> io::Result<()> {
    let json = serde_json::to_vec(&Settings {
        lock_shortcut: Some(accel.to_string()),
    })
    .map_err(io::Error::other)?;
    persist::write_atomically(dir, FILE_NAME, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tauri_plugin_global_shortcut::Code;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("a2m-hotkey-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn parses_keyboard_event_codes() {
        let shortcut = parse("Ctrl+Shift+KeyL").unwrap();
        assert_eq!(
            shortcut,
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyL)
        );
        assert!(parse("Alt+Digit1").is_ok());
        assert!(parse("Ctrl+F5").is_ok());
        assert!(parse("Ctrl+Alt+Numpad3").is_ok());
    }

    #[test]
    fn rejects_bare_keys_and_garbage() {
        assert!(parse("KeyL").is_err());
        assert!(parse("F5").is_err());
        assert!(parse("Ctrl+").is_err());
        assert!(parse("Ctrl+Banana").is_err());
    }

    #[test]
    fn display_is_human_readable() {
        assert_eq!(display("Ctrl+Shift+KeyL"), "Ctrl+Shift+L");
        assert_eq!(display("Alt+Digit1"), "Alt+1");
        assert_eq!(display("Ctrl+F5"), "Ctrl+F5");
        assert_eq!(display("Super+KeyK"), "Win+K");
        assert_eq!(display("Ctrl+Key"), "Ctrl+Key");
    }

    #[test]
    fn save_load_roundtrip_and_fallback() {
        let dir = temp_dir("roundtrip");
        assert_eq!(load(&dir), DEFAULT);
        save(&dir, "Alt+Digit1").unwrap();
        assert_eq!(load(&dir), "Alt+Digit1");
        fs::write(dir.join(FILE_NAME), r#"{"lock_shortcut":"KeyQ"}"#).unwrap();
        assert_eq!(load(&dir), DEFAULT, "an invalid saved shortcut falls back");
        let _ = fs::remove_dir_all(&dir);
    }
}
