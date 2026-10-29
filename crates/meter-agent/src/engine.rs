use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use meter_core::combat::Meter;
use meter_core::opcodes;
use meter_core::parser::{self, GameEvent};
use meter_core::pipeline::GamePacket;
use meter_proto::{LimitView, LimitsView, Snapshot, Status};

use crate::income::Income;
use crate::pet_names;
use crate::pets::Pets;
use crate::sources;

const PUBLISH_INTERVAL: Duration = Duration::from_millis(200);
const MS_PER_SECOND: i64 = 1_000;
const DAILY_KINAH_LIMIT: u64 = 1_000_000;
const DAILY_GATHERING_LIMIT: u64 = 1_000;

pub struct AppState {
    meter: Meter,
    status: Status,
    pets: Pets,
    limits: LimitsView,
    income: Income,
    changed: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            meter: Meter::default(),
            status: Status::default(),
            pets: Pets::default(),
            limits: LimitsView::default(),
            income: Income::default(),
            changed: true,
        }
    }
}

impl AppState {
    pub fn apply(&mut self, packet: &GamePacket, events: &[GameEvent]) {
        let ts_ms = packet.ts_ms();
        self.status.packets += 1;
        if packet.opcode == opcodes::HEARTBEAT {
            self.status.heartbeats += 1;
        }
        self.meter.tick(ts_ms);
        self.income.tick(ts_ms);
        for event in events {
            match event {
                GameEvent::Hit(_) => {
                    self.status.damage_records += 1;
                    self.meter.on_event(ts_ms, event);
                }
                GameEvent::PetCollection { levels, souls } => {
                    self.pets.on_collection(levels, souls)
                }
                GameEvent::PetSouls(picked) => self.pets.on_souls(ts_ms, picked),
                GameEvent::KinahBalance(balance) => self.income.kina.on_balance(ts_ms, *balance),
                GameEvent::DailyKinah(current) => {
                    self.limits.kinah = Some(LimitView {
                        current: *current,
                        limit: DAILY_KINAH_LIMIT,
                    })
                }
                GameEvent::DailyGathering(current) => {
                    self.limits.gathering = Some(LimitView {
                        current: *current,
                        limit: DAILY_GATHERING_LIMIT,
                    })
                }
                GameEvent::AbyssPoints { current, limit } => {
                    self.income.abyss.on_balance(ts_ms, *current);
                    self.limits.abyss = Some(LimitView {
                        current: *current,
                        limit: *limit,
                    })
                }
                _ => self.meter.on_event(ts_ms, event),
            }
        }
        self.changed = true;
    }

    pub fn set_pinned_pets(&mut self, ids: Vec<u32>) {
        self.pets.set_pinned(ids);
        self.changed = true;
    }

    pub fn set_pet_names(&mut self, names: HashMap<u32, String>) {
        self.pets.set_names(names);
        self.changed = true;
    }

    pub fn reset(&mut self) {
        self.meter.reset();
        self.changed = true;
    }

    pub fn set_idle_secs(&mut self, secs: u32) {
        self.meter.set_idle_ms(i64::from(secs) * MS_PER_SECOND);
        self.changed = true;
    }

    pub fn set_recent_secs(&mut self, secs: u32) {
        self.meter
            .set_recent_window_ms(i64::from(secs) * MS_PER_SECOND);
        self.changed = true;
    }

    pub fn reset_income(&mut self) {
        self.income.reset();
        self.changed = true;
    }

    pub fn take_changed(&mut self) -> Option<Snapshot> {
        std::mem::take(&mut self.changed).then(|| self.snapshot())
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut status = self.status.clone();
        status.names = self.meter.known_names() as u64;
        status.party_size = self.meter.party_size() as u64;
        status.pet_collection = self.pets.loaded();
        status.dots_dropped = self.meter.dropped_dots();
        status.idle_ms = self.meter.idle_ms();
        status.recent_window_ms = self.meter.recent_window_ms();
        Snapshot {
            status,
            encounter: self.meter.view(),
            session: self.meter.session_view(),
            pets: self.pets.view(),
            limits: self.limits,
            income: self.income.view(),
        }
    }
}

type Shared = Arc<Mutex<AppState>>;

fn lock(shared: &Shared) -> MutexGuard<'_, AppState> {
    shared.lock().expect("state lock")
}

pub enum Source {
    Live { record: Option<PathBuf> },
    Replay { file: PathBuf, speed: f64 },
}

impl Source {
    fn label(&self) -> String {
        match self {
            Source::Live { .. } => "live".to_string(),
            Source::Replay { file, .. } => format!(
                "replay: {}",
                file.file_name()
                    .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
            ),
        }
    }
}

pub struct Engine {
    state: Shared,
    stop: Arc<AtomicBool>,
}

impl Engine {
    pub fn start(source: Source, publish: impl Fn(Snapshot) + Send + 'static) -> Self {
        let state: Shared = Arc::new(Mutex::new(AppState {
            status: Status {
                source: source.label(),
                ..Status::default()
            },
            ..AppState::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        spawn_pet_name_download(state.clone());
        spawn_source_worker(state.clone(), stop.clone(), source);
        spawn_publisher(state.clone(), stop.clone(), publish);
        Self { state, stop }
    }

    pub fn state(&self) -> MutexGuard<'_, AppState> {
        lock(&self.state)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn spawn_pet_name_download(state: Shared) {
    std::thread::spawn(move || {
        let names = pet_names::fetch_names();
        if names.is_empty() {
            eprintln!("pet names: download failed, showing pet ids");
        }
        lock(&state).set_pet_names(names);
    });
}

fn spawn_source_worker(state: Shared, stop: Arc<AtomicBool>, source: Source) {
    std::thread::spawn(move || {
        let mut events = Vec::new();
        let on_packet = |packet: &GamePacket| {
            events.clear();
            parser::parse(packet.opcode, &packet.body, &mut events);
            lock(&state).apply(packet, &events);
        };
        let result = match source {
            Source::Live { record } => sources::live(&stop, record.as_deref(), None, on_packet),
            Source::Replay { file, speed } => sources::replay(&file, speed, &stop, on_packet),
        };
        match result {
            Ok(()) => eprintln!("source finished"),
            Err(error) if sources::is_interrupted(&error) => {}
            Err(error) => eprintln!("source error: {error:#}"),
        }
    });
}

fn spawn_publisher(
    state: Shared,
    stop: Arc<AtomicBool>,
    publish: impl Fn(Snapshot) + Send + 'static,
) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let changed = lock(&state).take_changed();
            if let Some(snapshot) = changed {
                publish(snapshot);
            }
            std::thread::sleep(PUBLISH_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use meter_core::net::FlowKey;

    const BLACK_SPIRIT: u32 = 1214;

    fn packet(ts_us: i64, opcode: u16) -> GamePacket {
        let addr = "10.0.0.1:1".parse().unwrap();
        GamePacket {
            ts_us,
            flow: FlowKey {
                src: addr,
                dst: addr,
            },
            opcode,
            body: Vec::new(),
        }
    }

    #[test]
    fn apply_counts_packets_and_builds_encounter() {
        let mut state = AppState::default();
        state.apply(&packet(0, opcodes::HEARTBEAT), &[]);
        let hit = GameEvent::Hit(parser::Hit {
            actor: 1,
            target: 2,
            skill: 13030340,
            amount: 500,
            crit: false,
            back: false,
            dot: false,
            hits: 1,
        });
        state.apply(&packet(1_000_000, opcodes::DAMAGE), &[hit]);
        let snapshot = state.snapshot();
        assert_eq!(
            (
                snapshot.status.packets,
                snapshot.status.heartbeats,
                snapshot.status.damage_records
            ),
            (2, 1, 1)
        );
        assert_eq!(snapshot.encounter.unwrap().total_damage, 500);
    }

    #[test]
    fn pet_packets_reach_the_snapshot() {
        let mut state = AppState::default();
        state.apply(
            &packet(0, opcodes::PET_COLLECTION),
            &[GameEvent::PetCollection {
                levels: vec![(BLACK_SPIRIT, 2)],
                souls: vec![(BLACK_SPIRIT, 32)],
            }],
        );
        state.apply(
            &packet(1_000_000, opcodes::PET_SOULS),
            &[GameEvent::PetSouls(vec![(BLACK_SPIRIT, 1)])],
        );
        let snapshot = state.snapshot();
        let pet = &snapshot.pets[0];
        assert!(snapshot.status.pet_collection);
        assert_eq!(
            (pet.souls, pet.goal, pet.gained, pet.last_ms),
            (33, 75, 1, 1000)
        );
    }

    #[test]
    fn income_reaches_the_snapshot_and_resets() {
        let mut state = AppState::default();
        let abyss = |current| GameEvent::AbyssPoints {
            current,
            limit: 2_500_000,
        };
        let events = [
            GameEvent::KinahBalance(1_000),
            GameEvent::KinahBalance(1_415),
            abyss(39_666),
            abyss(39_774),
        ];
        state.apply(&packet(0, opcodes::INVENTORY_ITEM), &events);
        let income = state.snapshot().income;
        assert_eq!((income.kina.earned, income.abyss.earned), (415, 108));
        state.reset_income();
        let income = state.snapshot().income;
        assert_eq!((income.kina.earned, income.abyss.earned), (0, 0));
    }

    #[test]
    fn daily_limits_reach_the_snapshot() {
        let mut state = AppState::default();
        assert_eq!(state.snapshot().limits, LimitsView::default());
        state.apply(
            &packet(0, opcodes::DAILY_KINAH),
            &[
                GameEvent::DailyKinah(59_149),
                GameEvent::DailyGathering(698),
                GameEvent::AbyssPoints {
                    current: 39_666,
                    limit: 2_500_000,
                },
            ],
        );
        let limits = state.snapshot().limits;
        let pair = |view: Option<LimitView>| view.map(|v| (v.current, v.limit));
        assert_eq!(pair(limits.kinah), Some((59_149, DAILY_KINAH_LIMIT)));
        assert_eq!(pair(limits.gathering), Some((698, DAILY_GATHERING_LIMIT)));
        assert_eq!(pair(limits.abyss), Some((39_666, 2_500_000)));
    }

    #[test]
    fn changes_are_published_once() {
        let mut state = AppState::default();
        assert!(
            state.take_changed().is_some(),
            "first snapshot is always published"
        );
        assert!(state.take_changed().is_none());
        state.apply(&packet(0, opcodes::HEARTBEAT), &[]);
        assert!(state.take_changed().is_some());
        state.set_pinned_pets(vec![BLACK_SPIRIT]);
        assert!(state.take_changed().is_some());
        assert!(state.take_changed().is_none());
    }
}
