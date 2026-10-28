use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use meter_proto::PetView;

const SOUL_GOALS_BY_LEVEL: [u32; 3] = [5, 25, 75];
const MAXED_GOAL: u32 = 0;

pub fn goal(level: u8) -> u32 {
    SOUL_GOALS_BY_LEVEL
        .get(usize::from(level))
        .copied()
        .unwrap_or(MAXED_GOAL)
}

#[derive(Default, Clone, Copy)]
struct GainedSouls {
    amount: u32,
    last_ms: i64,
}

#[derive(Default)]
pub struct Pets {
    loaded: bool,
    levels: HashMap<u32, u8>,
    souls: HashMap<u32, u32>,
    gained: HashMap<u32, GainedSouls>,
    pinned: Vec<u32>,
    names: HashMap<u32, String>,
}

impl Pets {
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn set_names(&mut self, names: HashMap<u32, String>) {
        self.names = names;
    }

    pub fn set_pinned(&mut self, ids: Vec<u32>) {
        let mut seen = HashSet::new();
        self.pinned = ids.into_iter().filter(|id| seen.insert(*id)).collect();
    }

    pub fn on_collection(&mut self, levels: &[(u32, u8)], souls: &[(u32, u32)]) {
        self.levels = levels.iter().copied().collect();
        self.souls = souls.iter().copied().collect();
        self.loaded = true;
    }

    pub fn on_souls(&mut self, ts_ms: i64, picked: &[(u32, u32)]) {
        for &(id, amount) in picked {
            let gained = self.gained.entry(id).or_default();
            gained.amount += amount;
            gained.last_ms = ts_ms;
            if self.loaded {
                *self.souls.entry(id).or_default() += amount;
            }
        }
    }

    fn view_of(&self, id: u32) -> PetView {
        let level = self.levels.get(&id).copied().unwrap_or(0);
        let gained = self.gained.get(&id).copied().unwrap_or_default();
        PetView {
            id,
            name: self.names.get(&id).cloned(),
            pinned: self.pinned.contains(&id),
            level,
            level_known: self.loaded,
            souls: self.souls.get(&id).copied().unwrap_or(0),
            goal: if self.loaded { goal(level) } else { 0 },
            gained: gained.amount,
            last_ms: gained.last_ms,
        }
    }

    pub fn view(&self) -> Vec<PetView> {
        let mut views: Vec<PetView> = self.pinned.iter().map(|&id| self.view_of(id)).collect();
        let mut recent: Vec<(u32, i64)> = self
            .gained
            .iter()
            .filter(|(id, _)| !self.pinned.contains(id))
            .map(|(&id, gained)| (id, gained.last_ms))
            .collect();
        recent.sort_by_key(|&(_, last_ms)| Reverse(last_ms));
        views.extend(recent.into_iter().map(|(id, _)| self.view_of(id)));
        views
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK_SPIRIT: u32 = 1214;
    const GNAT: u32 = 1183;
    const KRALL: u32 = 1213;

    fn row(pet: &PetView) -> (u32, bool, bool, u8, u32, u32, u32) {
        (
            pet.id,
            pet.pinned,
            pet.level_known,
            pet.level,
            pet.souls,
            pet.goal,
            pet.gained,
        )
    }

    #[test]
    fn collection_from_login_plus_picked_souls() {
        let mut pets = Pets::default();
        pets.on_collection(
            &[(BLACK_SPIRIT, 2), (KRALL, 1)],
            &[(BLACK_SPIRIT, 20), (KRALL, 4)],
        );
        assert!(pets.loaded());
        assert!(pets.view().is_empty(), "only pets with souls this session");
        pets.on_souls(5_000, &[(BLACK_SPIRIT, 1)]);
        pets.on_souls(6_000, &[(GNAT, 2)]);
        let views = pets.view();
        assert_eq!(
            row(&views[0]),
            (GNAT, false, true, 0, 2, 5, 2),
            "newest first, not owned yet"
        );
        assert_eq!(row(&views[1]), (BLACK_SPIRIT, false, true, 2, 21, 75, 1));
    }

    #[test]
    fn pinned_pets_stay_on_top_even_without_souls() {
        let mut pets = Pets::default();
        pets.on_collection(&[(KRALL, 1)], &[(KRALL, 4)]);
        pets.set_names(HashMap::from([(KRALL, "Krall".to_string())]));
        pets.set_pinned(vec![KRALL, KRALL]);
        pets.on_souls(1_000, &[(GNAT, 1)]);
        let views = pets.view();
        assert_eq!(row(&views[0]), (KRALL, true, true, 1, 4, 25, 0));
        assert_eq!(views[0].name.as_deref(), Some("Krall"));
        assert_eq!((views.len(), views[1].id), (2, GNAT));
    }

    #[test]
    fn without_the_login_collection_nothing_is_guessed() {
        let mut pets = Pets::default();
        pets.on_souls(1_000, &[(BLACK_SPIRIT, 1)]);
        assert!(!pets.loaded());
        assert_eq!(
            row(&pets.view()[0]),
            (BLACK_SPIRIT, false, false, 0, 0, 0, 1)
        );
    }

    #[test]
    fn goals_grow_per_level_and_stop_at_max() {
        assert_eq!([0, 1, 2, 3, 9].map(goal), [5, 25, 75, 0, 0]);
    }
}
