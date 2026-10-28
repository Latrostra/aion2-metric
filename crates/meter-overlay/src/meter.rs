use std::path::PathBuf;

use meter_agent::engine::{Engine, Source};
use meter_proto::Snapshot;
use tauri::{AppHandle, Emitter};

pub const SNAPSHOT_EVENT: &str = "snapshot";
const REPLAY_FLAG: &str = "--replay";
const SPEED_FLAG: &str = "--speed";
const REAL_TIME: f64 = 1.0;

pub struct MeterEngine(pub Engine);

fn source_from_args() -> Source {
    let args: Vec<String> = std::env::args().collect();
    let value_after = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
    };
    match value_after(REPLAY_FLAG) {
        Some(file) => Source::Replay {
            file: PathBuf::from(file),
            speed: value_after(SPEED_FLAG)
                .and_then(|speed| speed.parse().ok())
                .unwrap_or(REAL_TIME),
        },
        None => Source::Live { record: None },
    }
}

pub fn start(app: &AppHandle) -> MeterEngine {
    let app = app.clone();
    MeterEngine(Engine::start(
        source_from_args(),
        move |snapshot: Snapshot| {
            let _ = app.emit(SNAPSHOT_EVENT, snapshot);
        },
    ))
}
