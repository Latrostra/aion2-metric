use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use meter_proto::{ActorRow, EncounterView, SkillRow};

use crate::parser::{GameEvent, Hit, PartyMember, base_skill, is_class_skill};

pub const IDLE_MS: i64 = 15_000;
pub const MIN_IDLE_MS: i64 = 3_000;
pub const MAX_IDLE_MS: i64 = 300_000;
pub const DOT_CAST_WINDOW_MS: i64 = 60_000;
pub const UNOWNED_SUMMONS_ID: u32 = 0;
pub const UNOWNED_SUMMONS_NAME: &str = "Pety/summony";
pub const RECENT_WINDOW_MS: i64 = 10_000;
pub const MIN_RECENT_WINDOW_MS: i64 = 3_000;
pub const MAX_RECENT_WINDOW_MS: i64 = 60_000;
const CAST_LINK_BEFORE_MS: i64 = 3_000;
const CAST_LINK_AFTER_MS: i64 = 300;
const RECENT_CASTS_MS: i64 = 6_000;
const CAST_VARIANT_DIVISOR: u32 = 10;
const MAX_OWNER_CHAIN: usize = 4;
const TOP_SKILLS: usize = 5;
const MS_PER_SECOND: i64 = 1_000;
const MIN_DURATION_MS: i64 = 1_000;

fn second_of(ts_ms: i64) -> i64 {
    ts_ms.div_euclid(MS_PER_SECOND)
}

fn ms_to_secs(ms: i64) -> f64 {
    ms as f64 / MS_PER_SECOND as f64
}

fn fraction(part: u64, whole: u64) -> f64 {
    if whole > 0 {
        part as f64 / whole as f64
    } else {
        0.0
    }
}

#[derive(Default, Clone, Copy)]
struct SkillTotals {
    damage: u64,
    hits: u32,
}

#[derive(Default, Clone)]
struct ActorStats {
    damage: u64,
    hits: u32,
    records: u32,
    crits: u32,
    max_hit: u32,
    skills: HashMap<u32, SkillTotals>,
    damage_per_second: BTreeMap<i64, u64>,
}

impl ActorStats {
    fn add_hit(&mut self, ts_ms: i64, hit: &Hit) {
        let amount = u64::from(hit.amount);
        self.damage += amount;
        self.hits += hit.hits;
        self.records += 1;
        self.crits += u32::from(hit.crit);
        self.max_hit = self.max_hit.max(hit.amount);
        *self.damage_per_second.entry(second_of(ts_ms)).or_default() += amount;
        let skill = self.skills.entry(base_skill(hit.skill)).or_default();
        skill.damage += amount;
        skill.hits += hit.hits;
    }

    fn active_ms(&self, break_ms: i64) -> i64 {
        let seconds = self.damage_per_second.keys();
        let total: i64 = seconds
            .clone()
            .zip(seconds.skip(1))
            .map(|(previous, next)| (next - previous) * MS_PER_SECOND)
            .filter(|&gap| gap <= break_ms)
            .sum();
        total.max(MIN_DURATION_MS)
    }

    fn recent_damage(&self, now_second: i64, window_seconds: i64) -> u64 {
        self.damage_per_second
            .range(now_second - window_seconds + 1..=now_second)
            .map(|(_, damage)| damage)
            .sum()
    }

    fn merge(&mut self, other: &ActorStats) {
        for (&second, &damage) in &other.damage_per_second {
            *self.damage_per_second.entry(second).or_default() += damage;
        }
        self.damage += other.damage;
        self.hits += other.hits;
        self.records += other.records;
        self.crits += other.crits;
        self.max_hit = self.max_hit.max(other.max_hit);
        for (&skill, totals) in &other.skills {
            let merged = self.skills.entry(skill).or_default();
            merged.damage += totals.damage;
            merged.hits += totals.hits;
        }
    }

    fn top_skills(&self) -> Vec<SkillRow> {
        let mut skills: Vec<SkillRow> = self
            .skills
            .iter()
            .map(|(&skill, totals)| SkillRow {
                skill,
                damage: totals.damage,
                hits: totals.hits,
            })
            .collect();
        skills.sort_by_key(|skill| Reverse(skill.damage));
        skills.truncate(TOP_SKILLS);
        skills
    }
}

struct Encounter {
    start_ms: i64,
    last_ms: i64,
    active_ms: i64,
    actors: HashMap<u32, ActorStats>,
}

impl Encounter {
    fn new(ts_ms: i64) -> Self {
        Self {
            start_ms: ts_ms,
            last_ms: ts_ms,
            active_ms: 0,
            actors: HashMap::new(),
        }
    }

    fn record(&mut self, ts_ms: i64, hit: &Hit, idle_ms: i64) {
        let gap = ts_ms - self.last_ms;
        if (1..=idle_ms).contains(&gap) {
            self.active_ms += gap;
        }
        self.last_ms = self.last_ms.max(ts_ms);
        self.actors
            .entry(hit.actor)
            .or_default()
            .add_hit(ts_ms, hit);
    }
}

#[derive(Default)]
struct FoldedActor {
    stats: ActorStats,
    summon_damage: u64,
}

struct RowContext {
    duration_ms: i64,
    idle_ms: i64,
    now_second: i64,
    window_seconds: i64,
    total_damage: u64,
}

struct PendingSummon {
    spawn_ms: i64,
    owner_name: Option<String>,
    link_tried: bool,
}

#[derive(Clone, Copy)]
struct RecentCast {
    ts_ms: i64,
    actor: u32,
    skill: u32,
}

#[derive(Default)]
pub struct Meter {
    names: HashMap<u32, String>,
    self_ids: HashSet<u32>,
    player_like: HashSet<u32>,
    fight: Option<Encounter>,
    session: Option<Encounter>,
    idle_override_ms: Option<i64>,
    recent_window_override_ms: Option<i64>,
    now_ms: i64,
    owners: HashMap<u32, u32>,
    pending_summons: HashMap<u32, PendingSummon>,
    last_skill_use: HashMap<(u32, u32), i64>,
    recent_casts: VecDeque<RecentCast>,
    dropped_dots: u64,
    map_id: Option<u32>,
    party: HashMap<u32, String>,
    npc_templates: HashMap<u32, u32>,
    targets_of_self: HashSet<u32>,
    killed_templates: Vec<u32>,
}

impl Meter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick(&mut self, ts_ms: i64) {
        self.now_ms = self.now_ms.max(ts_ms);
    }

    pub fn dropped_dots(&self) -> u64 {
        self.dropped_dots
    }

    pub fn known_names(&self) -> usize {
        self.names.len()
    }

    pub fn take_kills(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.killed_templates)
    }

    pub fn party_size(&self) -> usize {
        self.party.len()
    }

    pub fn on_event(&mut self, ts_ms: i64, event: &GameEvent) {
        self.tick(ts_ms);
        match event {
            GameEvent::Identity { id, name, is_self } => self.on_identity(*id, name, *is_self),
            GameEvent::Summon {
                id,
                owner,
                owner_name,
            } => self.on_summon(ts_ms, *id, *owner, owner_name.as_deref()),
            GameEvent::Cast {
                actor,
                target,
                skill,
            } => self.on_cast(ts_ms, *actor, *target, *skill),
            GameEvent::Hit(hit) => self.on_hit(ts_ms, hit),
            GameEvent::MapLoad { map } => self.on_map_load(*map),
            GameEvent::SelfId { id } => self.on_self_id(*id),
            GameEvent::Caster { id, name } => self.on_caster(*id, name),
            GameEvent::Party { members } => self.set_party(members),
            GameEvent::PartyJoin(member) => {
                self.party.insert(member.char_id, member.name.clone());
            }
            GameEvent::PartyMemberLeft { char_id } => {
                self.party.remove(char_id);
            }
            GameEvent::PartyLeft => self.party.clear(),
            GameEvent::NpcSpawn { id, template } => {
                self.npc_templates.insert(*id, *template);
            }
            GameEvent::Death { id } => self.on_death(*id),
            GameEvent::PetCollection { .. }
            | GameEvent::PetSouls(_)
            | GameEvent::KinahBalance(_)
            | GameEvent::DailyKinah(_)
            | GameEvent::DailyGathering(_)
            | GameEvent::AbyssPoints { .. } => {}
        }
    }

    pub fn reset(&mut self) {
        self.fight = None;
        self.session = None;
    }

    fn on_self_id(&mut self, id: u32) {
        self.player_like.insert(id);
        self.owners.remove(&id);
        self.pending_summons.remove(&id);
        self.self_ids.insert(id);
    }

    fn on_caster(&mut self, id: u32, name: &str) {
        let is_summon = self.owners.contains_key(&id) || self.pending_summons.contains_key(&id);
        if !is_summon {
            self.on_identity(id, name, false);
        }
    }

    fn set_party(&mut self, members: &[PartyMember]) {
        self.party = members
            .iter()
            .map(|member| (member.char_id, member.name.clone()))
            .collect();
    }

    fn on_death(&mut self, id: u32) {
        let template = self.npc_templates.remove(&id);
        if self.targets_of_self.remove(&id)
            && let Some(template) = template
        {
            self.killed_templates.push(template);
        }
    }

    fn on_map_load(&mut self, map: u32) {
        if self.map_id.is_some_and(|current| current != map) {
            self.reset();
            self.names.clear();
            self.self_ids.clear();
            self.player_like.clear();
            self.owners.clear();
            self.pending_summons.clear();
            self.last_skill_use.clear();
            self.recent_casts.clear();
            self.npc_templates.clear();
            self.targets_of_self.clear();
        }
        self.map_id = Some(map);
    }

    fn on_identity(&mut self, id: u32, name: &str, is_self: bool) {
        self.names.insert(id, name.to_owned());
        self.player_like.insert(id);
        if is_self {
            self.self_ids.insert(id);
        }
        self.owners.remove(&id);
        self.pending_summons.remove(&id);
        let summons_of_this_owner: Vec<u32> = self
            .pending_summons
            .iter()
            .filter(|(_, pending)| pending.owner_name.as_deref() == Some(name))
            .map(|(&summon, _)| summon)
            .collect();
        for summon in summons_of_this_owner {
            self.link(summon, id);
        }
    }

    fn on_summon(&mut self, ts_ms: i64, id: u32, owner: Option<u32>, owner_name: Option<&str>) {
        if self.names.contains_key(&id) {
            return;
        }
        let owner_by_name = owner_name.and_then(|name| self.player_by_name(name));
        match owner.or(owner_by_name) {
            Some(owner) if owner != id => self.link(id, owner),
            _ => {
                self.pending_summons.insert(
                    id,
                    PendingSummon {
                        spawn_ms: ts_ms,
                        owner_name: owner_name.map(str::to_owned),
                        link_tried: false,
                    },
                );
            }
        }
    }

    fn on_cast(&mut self, ts_ms: i64, actor: u32, target: u32, skill: u32) {
        self.note_use(actor, skill, ts_ms);
        if self.pending_summons.contains_key(&actor) {
            self.link_by_cast_variant(actor, target, skill);
            return;
        }
        if self.owners.contains_key(&actor) {
            return;
        }
        if is_class_skill(skill) {
            self.player_like.insert(actor);
        }
        self.remember_cast(RecentCast {
            ts_ms,
            actor,
            skill,
        });
    }

    fn link_by_cast_variant(&mut self, summon: u32, target: u32, skill: u32) {
        let Some(pending) = self.pending_summons.get_mut(&summon) else {
            return;
        };
        if target != summon || pending.link_tried {
            return;
        }
        pending.link_tried = true;
        let spawn_ms = pending.spawn_ms;
        let variant = skill / CAST_VARIANT_DIVISOR;
        let caster = self
            .recent_casts
            .iter()
            .rev()
            .filter(|cast| cast.ts_ms <= spawn_ms + CAST_LINK_AFTER_MS)
            .take_while(|cast| cast.ts_ms >= spawn_ms - CAST_LINK_BEFORE_MS)
            .find(|cast| cast.skill / CAST_VARIANT_DIVISOR == variant)
            .map(|cast| cast.actor);
        if let Some(owner) = caster {
            self.link(summon, owner);
        }
    }

    fn remember_cast(&mut self, cast: RecentCast) {
        self.recent_casts.push_back(cast);
        while self
            .recent_casts
            .front()
            .is_some_and(|oldest| oldest.ts_ms < cast.ts_ms - RECENT_CASTS_MS)
        {
            self.recent_casts.pop_front();
        }
    }

    fn on_hit(&mut self, ts_ms: i64, hit: &Hit) {
        if hit.actor == hit.target {
            return;
        }
        if self.names.contains_key(&hit.target) || self.owners.contains_key(&hit.target) {
            return;
        }
        if self.self_ids.contains(&self.resolve_owner(hit.actor)) {
            self.targets_of_self.insert(hit.target);
        }
        if hit.dot {
            if !self.used_recently(hit.actor, hit.skill, ts_ms) {
                self.dropped_dots += 1;
                return;
            }
        } else {
            self.note_use(hit.actor, hit.skill, ts_ms);
        }
        let known_summon = self.owners.contains_key(&hit.actor);
        if !known_summon && !is_class_skill(hit.skill) {
            return;
        }
        if !known_summon && !self.pending_summons.contains_key(&hit.actor) {
            self.player_like.insert(hit.actor);
        }
        self.record_hit(ts_ms, hit);
    }

    fn record_hit(&mut self, ts_ms: i64, hit: &Hit) {
        let idle_ms = self.idle_ms();
        if self
            .fight
            .as_ref()
            .is_none_or(|fight| ts_ms - fight.last_ms > idle_ms)
        {
            self.fight = Some(Encounter::new(ts_ms));
        }
        if let Some(fight) = self.fight.as_mut() {
            fight.record(ts_ms, hit, idle_ms);
        }
        self.session
            .get_or_insert_with(|| Encounter::new(ts_ms))
            .record(ts_ms, hit, IDLE_MS);
    }

    fn note_use(&mut self, actor: u32, skill: u32, ts_ms: i64) {
        self.last_skill_use
            .insert((actor, base_skill(skill)), ts_ms);
    }

    fn used_recently(&self, actor: u32, skill: u32, ts_ms: i64) -> bool {
        self.last_skill_use
            .get(&(actor, base_skill(skill)))
            .is_some_and(|&used_ms| ts_ms - used_ms <= DOT_CAST_WINDOW_MS)
    }

    fn link(&mut self, summon: u32, owner: u32) {
        if summon == owner || self.names.contains_key(&summon) {
            return;
        }
        self.pending_summons.remove(&summon);
        self.owners.insert(summon, owner);
    }

    fn player_by_name(&self, name: &str) -> Option<u32> {
        self.names
            .iter()
            .find(|(_, known)| known.as_str() == name)
            .map(|(&id, _)| id)
    }

    fn resolve_owner(&self, id: u32) -> u32 {
        let mut resolved = id;
        for _ in 0..MAX_OWNER_CHAIN {
            match self.owners.get(&resolved) {
                Some(&owner) => resolved = owner,
                None => break,
            }
        }
        resolved
    }

    fn is_party_member(&self, name: &str) -> bool {
        self.party.values().any(|member| member == name)
    }

    pub fn idle_ms(&self) -> i64 {
        self.idle_override_ms.unwrap_or(IDLE_MS)
    }

    pub fn set_idle_ms(&mut self, ms: i64) {
        self.idle_override_ms = Some(ms.clamp(MIN_IDLE_MS, MAX_IDLE_MS));
    }

    pub fn recent_window_ms(&self) -> i64 {
        self.recent_window_override_ms.unwrap_or(RECENT_WINDOW_MS)
    }

    pub fn set_recent_window_ms(&mut self, ms: i64) {
        self.recent_window_override_ms = Some(ms.clamp(MIN_RECENT_WINDOW_MS, MAX_RECENT_WINDOW_MS));
    }

    pub fn view(&self) -> Option<EncounterView> {
        self.fight
            .as_ref()
            .map(|fight| self.view_of(fight, self.idle_ms()))
    }

    pub fn session_view(&self) -> Option<EncounterView> {
        self.session
            .as_ref()
            .map(|session| self.view_of(session, IDLE_MS))
    }

    fn view_of(&self, encounter: &Encounter, idle_ms: i64) -> EncounterView {
        let now_second = second_of(self.now_ms);
        let encounter_seconds = now_second - second_of(encounter.start_ms) + 1;
        let folded = self.fold_into_owners(encounter);
        let context = RowContext {
            duration_ms: encounter.active_ms.max(MIN_DURATION_MS),
            idle_ms,
            now_second,
            window_seconds: (self.recent_window_ms() / MS_PER_SECOND)
                .min(encounter_seconds)
                .max(1),
            total_damage: folded.values().map(|actor| actor.stats.damage).sum(),
        };
        let mut rows: Vec<ActorRow> = folded
            .into_iter()
            .map(|(id, actor)| self.actor_row(id, actor, &context))
            .collect();
        rows.sort_by(|left, right| {
            left.is_summons
                .cmp(&right.is_summons)
                .then(right.damage.cmp(&left.damage))
                .then(left.id.cmp(&right.id))
        });
        EncounterView {
            active: self.now_ms - encounter.last_ms <= idle_ms,
            start_ms: encounter.start_ms,
            duration_ms: context.duration_ms,
            recent_window_ms: context.window_seconds * MS_PER_SECOND,
            total_damage: context.total_damage,
            rows,
        }
    }

    fn fold_into_owners(&self, encounter: &Encounter) -> HashMap<u32, FoldedActor> {
        let mut folded: HashMap<u32, FoldedActor> = HashMap::new();
        for (&raw_id, stats) in &encounter.actors {
            let row_id = if self.pending_summons.contains_key(&raw_id) {
                UNOWNED_SUMMONS_ID
            } else {
                self.resolve_owner(raw_id)
            };
            if row_id != UNOWNED_SUMMONS_ID && !self.player_like.contains(&row_id) {
                continue;
            }
            let actor = folded.entry(row_id).or_default();
            actor.stats.merge(stats);
            if row_id != raw_id && row_id != UNOWNED_SUMMONS_ID {
                actor.summon_damage += stats.damage;
            }
        }
        folded
    }

    fn actor_row(&self, id: u32, actor: FoldedActor, context: &RowContext) -> ActorRow {
        let FoldedActor {
            stats,
            summon_damage,
        } = actor;
        let is_summons = id == UNOWNED_SUMMONS_ID;
        let name = if is_summons {
            Some(UNOWNED_SUMMONS_NAME.to_owned())
        } else {
            self.names.get(&id).cloned()
        };
        let active_ms = stats.active_ms(context.idle_ms);
        let is_self = self.self_ids.contains(&id);
        let is_party = !is_self
            && name
                .as_deref()
                .is_some_and(|name| self.is_party_member(name));
        let recent_damage = stats.recent_damage(context.now_second, context.window_seconds);
        ActorRow {
            is_party,
            dps_recent: recent_damage as f64 / context.window_seconds as f64,
            dps_active: stats.damage as f64 / ms_to_secs(active_ms),
            active_ms,
            id,
            name,
            is_self,
            is_summons,
            damage: stats.damage,
            summon_damage,
            dps: stats.damage as f64 / ms_to_secs(context.duration_ms),
            share: fraction(stats.damage, context.total_damage),
            hits: stats.hits,
            crit_rate: fraction(u64::from(stats.crits), u64::from(stats.records)),
            max_hit: stats.max_hit,
            top_skills: stats.top_skills(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAYER_SKILL: u32 = 13030340;
    const OTHER_PLAYER_SKILL: u32 = 13070120;
    const NPC_SKILL: u32 = 1607570;
    const DOT_SKILL: u32 = 13730007;
    const SUMMON_SKILL: u32 = 2_100_000;
    const DEFAULT_TARGET: u32 = 99;

    fn base_hit(actor: u32, skill: u32, amount: u32) -> Hit {
        Hit {
            actor,
            target: DEFAULT_TARGET,
            skill,
            amount,
            crit: false,
            back: false,
            dot: false,
            hits: 1,
        }
    }

    fn hit_event(actor: u32, skill: u32, amount: u32, crit: bool, dot: bool) -> GameEvent {
        GameEvent::Hit(Hit {
            crit,
            dot,
            ..base_hit(actor, skill, amount)
        })
    }

    fn hit(actor: u32, skill: u32, amount: u32, crit: bool) -> GameEvent {
        hit_event(actor, skill, amount, crit, false)
    }

    fn dot(actor: u32, skill: u32, amount: u32) -> GameEvent {
        hit_event(actor, skill, amount, false, true)
    }

    fn hit_on(actor: u32, target: u32) -> GameEvent {
        GameEvent::Hit(Hit {
            target,
            ..base_hit(actor, PLAYER_SKILL, 100)
        })
    }

    fn identity(id: u32, name: &str, is_self: bool) -> GameEvent {
        GameEvent::Identity {
            id,
            name: name.into(),
            is_self,
        }
    }

    fn caster(id: u32, name: &str) -> GameEvent {
        GameEvent::Caster {
            id,
            name: name.into(),
        }
    }

    fn summon(id: u32, owner: Option<u32>, owner_name: Option<&str>) -> GameEvent {
        GameEvent::Summon {
            id,
            owner,
            owner_name: owner_name.map(Into::into),
        }
    }

    fn cast(actor: u32, target: u32, skill: u32) -> GameEvent {
        GameEvent::Cast {
            actor,
            target,
            skill,
        }
    }

    fn ids(view: &EncounterView) -> Vec<u32> {
        view.rows.iter().map(|row| row.id).collect()
    }

    fn row(view: &EncounterView, id: u32) -> &ActorRow {
        view.rows.iter().find(|row| row.id == id).unwrap()
    }

    #[test]
    fn dps_share_and_sorting() {
        let mut meter = Meter::new();
        meter.on_event(0, &hit(1, PLAYER_SKILL, 1000, true));
        meter.on_event(10_000, &hit(2, OTHER_PLAYER_SKILL, 3000, false));
        meter.on_event(10_000, &hit(1, PLAYER_SKILL, 1000, false));
        let view = meter.view().unwrap();
        assert_eq!(view.duration_ms, 10_000);
        assert_eq!(view.total_damage, 5000);
        assert_eq!(ids(&view), vec![2, 1]);
        assert_eq!(view.rows[0].dps, 300.0);
        assert_eq!(view.rows[1].share, 0.4);
        assert_eq!(view.rows[1].crit_rate, 0.5);
        assert_eq!(
            view.rows[1].top_skills,
            vec![SkillRow {
                skill: 13030000,
                damage: 2000,
                hits: 2
            }]
        );
        assert!(view.active);
    }

    #[test]
    fn npc_skills_and_self_heals_are_ignored() {
        let mut meter = Meter::new();
        meter.on_event(0, &hit(5, NPC_SKILL, 2000, false));
        let self_hit = GameEvent::Hit(Hit {
            target: 7,
            ..base_hit(7, PLAYER_SKILL, 50)
        });
        meter.on_event(0, &self_hit);
        assert!(meter.view().is_none());
    }

    #[test]
    fn idle_gap_starts_new_encounter_and_marks_inactive() {
        let mut meter = Meter::new();
        meter.on_event(0, &hit(1, PLAYER_SKILL, 1000, false));
        meter.tick(IDLE_MS + 1);
        assert!(!meter.view().unwrap().active);
        meter.on_event(IDLE_MS + 1, &hit(2, PLAYER_SKILL, 10, false));
        assert_eq!(ids(&meter.view().unwrap()), vec![2]);
    }

    #[test]
    fn names_and_self_flag() {
        let mut meter = Meter::new();
        meter.on_event(0, &identity(1, "Faelis", true));
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        let view = meter.view().unwrap();
        let player = &view.rows[0];
        assert_eq!(player.name.as_deref(), Some("Faelis"));
        assert!(player.is_self);
    }

    #[test]
    fn dot_without_recent_use_is_dropped() {
        let mut meter = Meter::new();
        meter.on_event(0, &dot(1, DOT_SKILL, 459));
        assert!(meter.view().is_none());
        assert_eq!(meter.dropped_dots(), 1);
    }

    #[test]
    fn dot_after_cast_of_same_family_counts() {
        let mut meter = Meter::new();
        meter.on_event(0, &cast(1, DEFAULT_TARGET, 13730005));
        meter.on_event(5_000, &dot(1, DOT_SKILL, 459));
        assert_eq!(meter.view().unwrap().total_damage, 459);
        meter.on_event(5_000 + DOT_CAST_WINDOW_MS + 1, &dot(1, DOT_SKILL, 459));
        assert_eq!(meter.view().unwrap().total_damage, 459);
        assert_eq!(meter.dropped_dots(), 1);
    }

    #[test]
    fn summon_with_owner_id_is_credited_to_owner() {
        let mut meter = Meter::new();
        meter.on_event(0, &summon(500, Some(1), None));
        meter.on_event(0, &hit(1, PLAYER_SKILL, 100, false));
        meter.on_event(0, &hit(500, SUMMON_SKILL, 40, false));
        let view = meter.view().unwrap();
        assert_eq!(ids(&view), vec![1]);
        assert_eq!((view.rows[0].damage, view.rows[0].summon_damage), (140, 40));
    }

    #[test]
    fn summon_owner_name_resolves_when_identity_arrives_later() {
        let mut meter = Meter::new();
        meter.on_event(0, &summon(500, None, Some("Kyra")));
        meter.on_event(0, &hit(500, PLAYER_SKILL, 40, false));
        assert_eq!(ids(&meter.view().unwrap()), vec![UNOWNED_SUMMONS_ID]);
        meter.on_event(1_000, &identity(2, "Kyra", false));
        let view = meter.view().unwrap();
        assert_eq!(ids(&view), vec![2]);
        assert_eq!(view.rows[0].name.as_deref(), Some("Kyra"));
    }

    #[test]
    fn effect_entity_linked_by_cast_variant() {
        let mut meter = Meter::new();
        meter.on_event(1_000, &cast(1, DEFAULT_TARGET, 15280140));
        meter.on_event(1_000, &cast(2, DEFAULT_TARGET, 15280240));
        meter.on_event(1_200, &summon(700, None, None));
        meter.on_event(1_500, &cast(700, 700, 15280141));
        meter.on_event(1_600, &hit(700, 15280141, 900, false));
        let view = meter.view().unwrap();
        assert_eq!(ids(&view), vec![1]);
        assert_eq!(view.rows[0].summon_damage, 900);
    }

    #[test]
    fn summon_of_a_mob_never_becomes_a_row() {
        let mut meter = Meter::new();
        meter.on_event(0, &summon(500, Some(77), None));
        meter.on_event(0, &hit(77, NPC_SKILL, 1000, false));
        meter.on_event(0, &hit(500, SUMMON_SKILL, 40, false));
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        assert_eq!(ids(&meter.view().unwrap()), vec![1]);
    }

    #[test]
    fn old_self_ids_stay_self_after_zone_change() {
        let mut meter = Meter::new();
        meter.on_event(0, &identity(1, "Me", true));
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        meter.on_event(1, &identity(2, "Me", true));
        assert!(meter.view().unwrap().rows[0].is_self);
    }

    #[test]
    fn same_map_reload_keeps_names() {
        let mut meter = Meter::new();
        meter.on_event(0, &GameEvent::MapLoad { map: 600022 });
        meter.on_event(0, &identity(1, "A", false));
        meter.on_event(1, &GameEvent::MapLoad { map: 600022 });
        assert_eq!(meter.known_names(), 1);
    }

    #[test]
    fn map_change_resets_the_meter_completely() {
        let mut meter = Meter::new();
        meter.on_event(0, &GameEvent::MapLoad { map: 600022 });
        meter.on_event(0, &identity(1, "A", true));
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        meter.on_event(1, &GameEvent::MapLoad { map: 1110 });
        assert_eq!(meter.known_names(), 0);
        assert!(
            meter.view().is_none(),
            "fight is gone right after the loading screen"
        );
        meter.on_event(2, &hit(1, PLAYER_SKILL, 7, false));
        let view = meter.view().unwrap();
        assert_eq!((view.total_damage, view.rows[0].name.clone()), (7, None));
    }

    #[test]
    fn map_total_survives_breaks_and_deaths_and_counts_only_fighting_time() {
        let mut meter = Meter::new();
        meter.on_event(0, &GameEvent::MapLoad { map: 600022 });
        meter.on_event(0, &hit(1, PLAYER_SKILL, 100, false));
        meter.on_event(10_000, &hit(1, PLAYER_SKILL, 100, false));
        meter.on_event(60_000, &GameEvent::MapLoad { map: 600022 });
        meter.on_event(130_000, &hit(1, PLAYER_SKILL, 100, false));
        meter.on_event(140_000, &hit(1, PLAYER_SKILL, 100, false));
        let fight = meter.view().unwrap();
        assert_eq!(
            (fight.total_damage, fight.duration_ms),
            (200, 10_000),
            "fight split"
        );
        let map = meter.session_view().unwrap();
        assert_eq!(
            (map.total_damage, map.duration_ms),
            (400, 20_000),
            "whole map, no break"
        );
        assert!((map.rows[0].dps - 20.0).abs() < 1e-9);
        meter.on_event(150_000, &GameEvent::MapLoad { map: 1110 });
        assert!(meter.view().is_none() && meter.session_view().is_none());
    }

    #[test]
    fn fight_timeout_is_configurable_and_clamped() {
        let mut meter = Meter::new();
        assert_eq!(meter.idle_ms(), IDLE_MS);
        meter.set_idle_ms(60_000);
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        meter.on_event(40_000, &hit(1, PLAYER_SKILL, 10, false));
        assert_eq!(meter.view().unwrap().total_damage, 20);
        meter.set_idle_ms(5_000);
        meter.on_event(46_000, &hit(1, PLAYER_SKILL, 7, false));
        assert_eq!(meter.view().unwrap().total_damage, 7);
        meter.set_idle_ms(1);
        assert_eq!(meter.idle_ms(), MIN_IDLE_MS);
        meter.set_idle_ms(i64::MAX);
        assert_eq!(meter.idle_ms(), MAX_IDLE_MS);
    }

    #[test]
    fn reset_button_clears_the_map_total_too() {
        let mut meter = Meter::new();
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        meter.reset();
        assert!(meter.session_view().is_none());
    }

    #[test]
    fn self_and_name_known_without_a_loading_screen() {
        let mut meter = Meter::new();
        meter.on_event(0, &hit(13131, PLAYER_SKILL, 50, false));
        assert_eq!(meter.view().unwrap().rows[0].name, None);
        meter.on_event(1, &GameEvent::SelfId { id: 13131 });
        meter.on_event(2, &caster(13131, "Me"));
        let view = meter.view().unwrap();
        let player = &view.rows[0];
        assert_eq!((player.name.as_deref(), player.is_self), (Some("Me"), true));
    }

    #[test]
    fn names_never_carry_over_into_a_new_zone() {
        let mut meter = Meter::new();
        meter.on_event(0, &GameEvent::MapLoad { map: 1 });
        meter.on_event(0, &GameEvent::SelfId { id: 7 });
        meter.on_event(0, &caster(7, "Me"));
        meter.on_event(1, &GameEvent::MapLoad { map: 2 });
        meter.on_event(2, &GameEvent::SelfId { id: 42 });
        meter.on_event(3, &hit(42, PLAYER_SKILL, 10, false));
        meter.on_event(3, &hit(7, PLAYER_SKILL, 10, false));
        let view = meter.view().unwrap();
        assert!(
            view.rows.iter().all(|player| player.name.is_none()),
            "no stale names"
        );
        assert!(
            view.rows
                .iter()
                .any(|player| player.id == 42 && player.is_self)
        );
        meter.on_event(4, &caster(42, "Me"));
        let view = meter.view().unwrap();
        assert_eq!(row(&view, 42).name.as_deref(), Some("Me"));
    }

    #[test]
    fn caster_name_of_a_summon_is_ignored() {
        let mut meter = Meter::new();
        meter.on_event(0, &summon(500, Some(1), None));
        meter.on_event(1, &caster(500, "Pet"));
        assert_eq!(meter.known_names(), 0);
    }

    #[test]
    fn party_members_are_flagged_by_name_and_survive_zone_changes() {
        let member = |char_id, name: &str| PartyMember {
            char_id,
            name: name.into(),
        };
        let mut meter = Meter::new();
        meter.on_event(0, &GameEvent::MapLoad { map: 1 });
        meter.on_event(
            0,
            &GameEvent::Party {
                members: vec![member(100, "Me"), member(200, "Friend")],
            },
        );
        meter.on_event(1, &GameEvent::MapLoad { map: 2 });
        meter.on_event(2, &GameEvent::SelfId { id: 42 });
        meter.on_event(2, &caster(42, "Me"));
        meter.on_event(2, &caster(7, "Friend"));
        meter.on_event(2, &caster(8, "Stranger"));
        for id in [42, 7, 8] {
            meter.on_event(3, &hit(id, PLAYER_SKILL, 100, false));
        }
        let flags = |meter: &Meter| {
            let view = meter.view().unwrap();
            [42, 7, 8].map(|id| {
                let player = row(&view, id);
                (player.is_self, player.is_party)
            })
        };
        assert_eq!(
            flags(&meter),
            [(true, false), (false, true), (false, false)]
        );
        meter.on_event(4, &GameEvent::PartyJoin(member(300, "Stranger")));
        assert_eq!(flags(&meter)[2], (false, true), "joined");
        meter.on_event(5, &GameEvent::PartyMemberLeft { char_id: 200 });
        assert_eq!(flags(&meter)[1], (false, false), "left");
        meter.on_event(6, &GameEvent::PartyLeft);
        assert_eq!(meter.party_size(), 0);
    }

    #[test]
    fn kills_count_monsters_the_player_or_their_pet_damaged() {
        let mut meter = Meter::new();
        meter.on_event(0, &GameEvent::SelfId { id: 42 });
        meter.on_event(0, &summon(500, Some(42), None));
        for (id, template) in [(1001, 2600076), (1002, 2600076), (1003, 2600069)] {
            meter.on_event(0, &GameEvent::NpcSpawn { id, template });
        }
        meter.on_event(1, &hit_on(42, 1001));
        meter.on_event(1, &hit_on(500, 1002));
        meter.on_event(1, &hit_on(7, 1003));
        for id in [1001, 1002, 1003, 1001] {
            meter.on_event(2, &GameEvent::Death { id });
        }
        assert_eq!(meter.take_kills(), vec![2600076, 2600076], "each kill once");
        assert!(meter.take_kills().is_empty());
    }

    #[test]
    fn current_dps_uses_the_last_window_only() {
        let mut meter = Meter::new();
        for second in 0..20 {
            meter.on_event(second * 1000, &hit(1, PLAYER_SKILL, 1000, false));
        }
        meter.on_event(29_000, &hit(1, PLAYER_SKILL, 0, false));
        meter.tick(29_500);
        assert_eq!(
            meter.view().unwrap().rows[0].dps_recent,
            0.0,
            "nothing in the last 10 s"
        );
        let mut young = Meter::new();
        young.on_event(0, &hit(1, PLAYER_SKILL, 3000, false));
        young.on_event(2_000, &hit(1, PLAYER_SKILL, 3000, false));
        let view = young.view().unwrap();
        assert_eq!(view.recent_window_ms, 3_000);
        assert!((view.rows[0].dps_recent - 2000.0).abs() < 1e-9);
        young.set_recent_window_ms(1);
        assert_eq!(young.recent_window_ms(), MIN_RECENT_WINDOW_MS);
        young.set_recent_window_ms(i64::MAX);
        assert_eq!(young.recent_window_ms(), MAX_RECENT_WINDOW_MS);
    }

    #[test]
    fn active_dps_ignores_the_players_own_idle_time() {
        let mut meter = Meter::new();
        for second in 0..=60 {
            if second <= 10 {
                meter.on_event(second * 1000, &hit(1, PLAYER_SKILL, 1000, false));
            }
            meter.on_event(second * 1000, &hit(2, OTHER_PLAYER_SKILL, 100, false));
        }
        let view = meter.view().unwrap();
        let first = row(&view, 1);
        let second = row(&view, 2);
        assert_eq!(view.duration_ms, 60_000);
        assert!(
            (first.dps - 11_000.0 / 60.0).abs() < 1e-6,
            "encounter DPS drops"
        );
        assert_eq!(first.active_ms, 10_000, "first to last hit");
        assert!(
            (first.dps_active - 1100.0).abs() < 1e-6,
            "active DPS does not"
        );
        assert_eq!(second.active_ms, 60_000);
    }

    #[test]
    fn active_time_counts_pauses_inside_a_fight_but_not_breaks_between_fights() {
        let mut meter = Meter::new();
        meter.on_event(0, &hit(1, PLAYER_SKILL, 100, false));
        meter.on_event(12_000, &hit(1, PLAYER_SKILL, 100, false));
        assert_eq!(meter.view().unwrap().rows[0].active_ms, 12_000);
        meter.on_event(52_000, &hit(1, PLAYER_SKILL, 100, false));
        meter.on_event(57_000, &hit(1, PLAYER_SKILL, 100, false));
        assert_eq!(meter.session_view().unwrap().rows[0].active_ms, 17_000);
        assert_eq!(meter.view().unwrap().rows[0].active_ms, 5_000, "new fight");
    }

    #[test]
    fn pet_damage_does_not_double_count_active_time() {
        let mut meter = Meter::new();
        meter.on_event(0, &summon(500, Some(1), None));
        for second in 0..5 {
            meter.on_event(second * 1000, &hit(1, PLAYER_SKILL, 100, false));
            meter.on_event(second * 1000, &hit(500, PLAYER_SKILL, 100, false));
        }
        let view = meter.view().unwrap();
        let owner = &view.rows[0];
        assert_eq!(
            (owner.damage, owner.active_ms),
            (1000, 4_000),
            "one timeline, not 2x"
        );
    }

    #[test]
    fn stage_teleport_keeps_the_fight() {
        let mut meter = Meter::new();
        meter.on_event(0, &GameEvent::MapLoad { map: 600022 });
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        meter.on_event(1, &GameEvent::MapLoad { map: 600022 });
        meter.on_event(2, &hit(1, PLAYER_SKILL, 5, false));
        assert_eq!(meter.view().unwrap().total_damage, 15);
    }

    #[test]
    fn reset_clears_the_fight_but_keeps_names() {
        let mut meter = Meter::new();
        meter.on_event(0, &identity(1, "A", false));
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        meter.reset();
        assert!(meter.view().is_none());
        meter.on_event(1, &hit(1, PLAYER_SKILL, 3, false));
        let view = meter.view().unwrap();
        assert_eq!(
            (view.total_damage, view.rows[0].name.as_deref()),
            (3, Some("A"))
        );
    }

    #[test]
    fn hits_on_players_and_their_pets_are_ignored() {
        let mut meter = Meter::new();
        meter.on_event(0, &identity(DEFAULT_TARGET, "Tank", false));
        meter.on_event(0, &hit(5, PLAYER_SKILL, 500, false));
        assert!(meter.view().is_none());
    }

    #[test]
    fn unowned_summons_are_listed_after_players() {
        let mut meter = Meter::new();
        meter.on_event(0, &summon(700, None, None));
        meter.on_event(0, &hit(700, PLAYER_SKILL, 9_000, false));
        meter.on_event(0, &hit(1, PLAYER_SKILL, 10, false));
        let view = meter.view().unwrap();
        assert_eq!(ids(&view), vec![1, UNOWNED_SUMMONS_ID]);
        assert!(view.rows[1].is_summons);
        assert_eq!(view.rows[1].name.as_deref(), Some(UNOWNED_SUMMONS_NAME));
    }
}
