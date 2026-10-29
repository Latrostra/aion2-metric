use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub status: Status,
    pub encounter: Option<EncounterView>,
    #[serde(default)]
    pub session: Option<EncounterView>,
    #[serde(default)]
    pub pets: Vec<PetView>,
    #[serde(default)]
    pub limits: LimitsView,
    #[serde(default)]
    pub income: IncomeView,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GainView {
    pub earned: u64,
    pub per_hour: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomeView {
    pub elapsed_secs: u64,
    pub kina: GainView,
    pub abyss: GainView,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitView {
    pub current: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitsView {
    pub kinah: Option<LimitView>,
    pub abyss: Option<LimitView>,
    pub gathering: Option<LimitView>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PetView {
    pub id: u32,
    pub name: Option<String>,
    pub pinned: bool,
    pub level: u8,
    pub level_known: bool,
    pub souls: u32,
    pub goal: u32,
    pub gained: u32,
    pub last_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub source: String,
    pub packets: u64,
    pub heartbeats: u64,
    pub damage_records: u64,
    pub names: u64,
    pub dots_dropped: u64,
    #[serde(default)]
    pub idle_ms: i64,
    #[serde(default)]
    pub recent_window_ms: i64,
    #[serde(default)]
    pub party_size: u64,
    #[serde(default)]
    pub pet_collection: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EncounterView {
    pub active: bool,
    pub start_ms: i64,
    pub duration_ms: i64,
    #[serde(default)]
    pub recent_window_ms: i64,
    pub total_damage: u64,
    pub rows: Vec<ActorRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ActorRow {
    pub id: u32,
    pub name: Option<String>,
    pub is_self: bool,
    #[serde(default)]
    pub is_party: bool,
    pub is_summons: bool,
    pub damage: u64,
    pub summon_damage: u64,
    pub dps: f64,
    #[serde(default)]
    pub dps_recent: f64,
    #[serde(default)]
    pub dps_active: f64,
    #[serde(default)]
    pub active_ms: i64,
    pub share: f64,
    pub hits: u32,
    pub crit_rate: f64,
    pub max_hit: u32,
    pub top_skills: Vec<SkillRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillRow {
    pub skill: u32,
    pub damage: u64,
    pub hits: u32,
}
