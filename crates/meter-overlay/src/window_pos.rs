use std::fs;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use serde::{Deserialize, Serialize};

use crate::persist;

pub const FILE_NAME: &str = "window.json";
pub const LEGACY_FILE_NAME: &str = ".window-state.json";
const LEGACY_MAIN_WINDOW_KEY: &str = "main";
const GRAB_POINT_OFFSET_X: i32 = 40;
const GRAB_POINT_OFFSET_Y: i32 = 15;
const SANE_WIDTHS: RangeInclusive<u32> = 100..=8000;
const SANE_HEIGHTS: RangeInclusive<u32> = 60..=8000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Area {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && i64::from(x) < i64::from(self.x) + i64::from(self.width)
            && i64::from(y) < i64::from(self.y) + i64::from(self.height)
    }
}

impl Geometry {
    pub fn is_reachable(&self, monitors: &[Area]) -> bool {
        let grab_x = self.x + GRAB_POINT_OFFSET_X;
        let grab_y = self.y + GRAB_POINT_OFFSET_Y;
        monitors
            .iter()
            .any(|monitor| monitor.contains(grab_x, grab_y))
    }

    fn is_sane(&self) -> bool {
        SANE_WIDTHS.contains(&self.width) && SANE_HEIGHTS.contains(&self.height)
    }
}

pub fn load(dir: &Path) -> Option<Geometry> {
    let current = fs::read_to_string(dir.join(FILE_NAME))
        .ok()
        .and_then(|json| serde_json::from_str::<Geometry>(&json).ok());
    let legacy = || {
        let json = fs::read_to_string(dir.join(LEGACY_FILE_NAME)).ok()?;
        let windows: serde_json::Value = serde_json::from_str(&json).ok()?;
        serde_json::from_value::<Geometry>(windows.get(LEGACY_MAIN_WINDOW_KEY)?.clone()).ok()
    };
    current.or_else(legacy).filter(Geometry::is_sane)
}

pub fn save(dir: &Path, geometry: &Geometry) -> io::Result<()> {
    let json = serde_json::to_vec(geometry).map_err(io::Error::other)?;
    persist::write_atomically(dir, FILE_NAME, &json)
}

pub struct Tracker {
    latest: Mutex<Option<Geometry>>,
    saver: Mutex<Option<Sender<Geometry>>>,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            latest: Mutex::new(None),
            saver: Mutex::new(None),
        }
    }

    pub fn start(&self, dir: PathBuf, initial: Geometry) {
        *self.latest.lock().expect("geometry lock") = Some(initial);
        let saver = persist::spawn_latest_value_saver(move |geometry: &Geometry| {
            if let Err(err) = save(&dir, geometry) {
                eprintln!("could not save window position: {err}");
            }
        });
        *self.saver.lock().expect("saver lock") = Some(saver);
    }

    pub fn moved(&self, x: i32, y: i32) {
        self.update(|geometry| {
            geometry.x = x;
            geometry.y = y;
        });
    }

    pub fn resized(&self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.update(|geometry| {
            geometry.width = width;
            geometry.height = height;
        });
    }

    fn update(&self, change: impl FnOnce(&mut Geometry)) {
        let updated = {
            let mut latest = self.latest.lock().expect("geometry lock");
            let Some(geometry) = latest.as_mut() else {
                return;
            };
            change(geometry);
            *geometry
        };
        if let Some(saver) = self.saver.lock().expect("saver lock").as_ref() {
            let _ = saver.send(updated);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("a2m-overlay-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    const SAVED: Geometry = Geometry {
        x: 1210,
        y: 75,
        width: 440,
        height: 360,
    };

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        save(&dir, &SAVED).unwrap();
        assert_eq!(load(&dir), Some(SAVED));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_the_old_plugin_file() {
        let dir = temp_dir("legacy");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(LEGACY_FILE_NAME),
            r#"{"main":{"width":440,"height":360,"x":1210,"y":75,"prev_x":1,"prev_y":2,
               "maximized":false,"visible":true,"decorated":true,"fullscreen":false}}"#,
        )
        .unwrap();
        assert_eq!(load(&dir), Some(SAVED));
        let moved = Geometry {
            x: 5,
            y: 6,
            ..SAVED
        };
        save(&dir, &moved).unwrap();
        assert_eq!(load(&dir), Some(moved));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_corrupt_or_absurd_files_are_ignored() {
        let dir = temp_dir("bad");
        assert_eq!(load(&dir), None);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE_NAME), "not json").unwrap();
        assert_eq!(load(&dir), None);
        save(&dir, &Geometry { width: 0, ..SAVED }).unwrap();
        assert_eq!(load(&dir), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reachable_only_on_an_existing_monitor() {
        let left = Area {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let right = Area {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        };
        assert!(SAVED.is_reachable(&[left]));
        let on_right = Geometry {
            x: 3000,
            y: 500,
            ..SAVED
        };
        assert!(on_right.is_reachable(&[left, right]));
        assert!(!on_right.is_reachable(&[left]), "second monitor unplugged");
        assert!(!Geometry { x: -2000, ..SAVED }.is_reachable(&[left, right]));
    }

    #[test]
    fn tracker_saves_every_move_immediately() {
        let dir = temp_dir("tracker");
        let tracker = Tracker::new();
        tracker.moved(1, 1);
        tracker.start(dir.clone(), SAVED);
        tracker.moved(300, 200);
        tracker.resized(500, 400);
        tracker.resized(0, 0);
        let want = Geometry {
            x: 300,
            y: 200,
            width: 500,
            height: 400,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while load(&dir) != Some(want) {
            assert!(Instant::now() < deadline, "saved {:?}", load(&dir));
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
