use std::str::FromStr;

use leptos::prelude::*;
use meter_proto::ActorRow;

use crate::dps::DpsKind;
use crate::i18n::Lang;

pub const LANG_KEY: &str = "a2m.lang";
pub const ACTIVE_PANEL_KEY: &str = "a2m.view";
pub const PINNED_PETS_KEY: &str = "a2m.pinned_pets";
pub const SETTINGS_TAB_KEY: &str = "a2m.settings_tab";
pub const HIDE_UNFOCUSED_KEY: &str = "a2m.hide_unfocused";
const BG_ALPHA_KEY: &str = "a2m.bg_alpha";
const TEXT_ALPHA_KEY: &str = "a2m.text_alpha";
const ROWS_IN_VIEW_KEY: &str = "a2m.visible_rows";
const SHOW_DETAILS_KEY: &str = "a2m.details";
const MAP_MODE_KEY: &str = "a2m.map_mode";
const IDLE_SECS_KEY: &str = "a2m.idle_secs";
const DPS_KIND_KEY: &str = "a2m.dps_kind";
const RECENT_SECS_KEY: &str = "a2m.recent_secs";
const SHOW_SELF_KEY: &str = "a2m.show_self";
const SHOW_PARTY_KEY: &str = "a2m.show_party";
const SHOW_OTHERS_KEY: &str = "a2m.show_others";
const SHOW_SUMMONS_KEY: &str = "a2m.show_summons";
const NAMED_ONLY_KEY: &str = "a2m.named_only";
const MAX_ROWS_KEY: &str = "a2m.max_rows";

const DEFAULT_COLORS: [(&str, &str); 4] = [
    ("a2m.color_me", "#f59e0b"),
    ("a2m.color_party", "#22c55e"),
    ("a2m.color_others", "#3b82f6"),
    ("a2m.color_summons", "#8b8f98"),
];
const HEX_COLOR_LEN: usize = 7;

const STORED_TRUE: &str = "1";
const STORED_FALSE: &str = "0";
const ID_SEPARATOR: &str = ",";

pub const DEFAULT_HIDE_UNFOCUSED: bool = true;
const DEFAULT_BG_ALPHA: f64 = 0.8;
const DEFAULT_TEXT_ALPHA: f64 = 1.0;
pub const ROWS_IN_VIEW_CHOICES: [u32; 6] = [3, 4, 5, 6, 8, 10];
const DEFAULT_ROWS_IN_VIEW: u32 = 5;
const DEFAULT_SHOW_DETAILS: bool = true;
const DEFAULT_MAP_MODE: bool = true;
pub const MIN_IDLE_SECS: u32 = 3;
pub const MAX_IDLE_SECS: u32 = 120;
const DEFAULT_IDLE_SECS: u32 = 15;
const DEFAULT_DPS_KIND: DpsKind = DpsKind::Average;
pub const MIN_RECENT_SECS: u32 = 3;
pub const MAX_RECENT_SECS: u32 = 60;
const DEFAULT_RECENT_SECS: u32 = 10;
const DEFAULT_SHOW_GROUP: bool = true;
const DEFAULT_NAMED_ONLY: bool = false;
pub const ALL_ROWS: usize = 0;
pub const MAX_ROWS_CHOICES: [usize; 5] = [ALL_ROWS, 3, 5, 8, 12];

pub fn load(key: &str) -> Option<String> {
    window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|storage| storage.get_item(key).ok().flatten())
}

pub fn save(key: &str, value: &str) {
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item(key, value);
    }
}

pub fn persisted<T: Send + Sync + 'static>(
    key: &'static str,
    initial: T,
    encode: impl Fn(&T) -> String + 'static,
) -> RwSignal<T> {
    let signal = RwSignal::new(initial);
    Effect::new(move |_| save(key, &signal.with(&encode)));
    signal
}

fn load_valid<T: FromStr>(key: &str, is_valid: impl Fn(&T) -> bool) -> Option<T> {
    load(key)
        .and_then(|value| value.parse().ok())
        .filter(is_valid)
}

pub fn bool_pref(key: &'static str, default: bool) -> RwSignal<bool> {
    let initial = load(key).map_or(default, |value| value == STORED_TRUE);
    persisted(key, initial, |&on| {
        if on { STORED_TRUE } else { STORED_FALSE }.to_string()
    })
}

fn number_pref<T>(key: &'static str, default: T, is_valid: impl Fn(&T) -> bool) -> RwSignal<T>
where
    T: FromStr + ToString + Send + Sync + 'static,
{
    let initial = load_valid(key, is_valid).unwrap_or(default);
    persisted(key, initial, T::to_string)
}

fn alpha_pref(key: &'static str, default: f64) -> RwSignal<f64> {
    let initial = load_valid(key, |alpha| (0.0..=1.0).contains(alpha)).unwrap_or(default);
    persisted(key, initial, |alpha| format!("{alpha:.2}"))
}

fn color_pref(key: &'static str, default: &'static str) -> RwSignal<String> {
    let is_hex_color = |value: &String| value.len() == HEX_COLOR_LEN && value.starts_with('#');
    let initial = load(key)
        .filter(is_hex_color)
        .unwrap_or_else(|| default.into());
    persisted(key, initial, String::clone)
}

pub fn load_lang() -> Lang {
    load(LANG_KEY)
        .as_deref()
        .and_then(Lang::parse)
        .unwrap_or(Lang::En)
}

pub fn join_ids(ids: &[u32]) -> String {
    ids.iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(ID_SEPARATOR)
}

pub fn load_pinned_pets() -> Vec<u32> {
    load(PINNED_PETS_KEY)
        .map(|ids| {
            ids.split(ID_SEPARATOR)
                .filter_map(|id| id.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn pinned_pets_pref() -> RwSignal<Vec<u32>> {
    persisted(PINNED_PETS_KEY, load_pinned_pets(), |ids| join_ids(ids))
}

#[derive(Clone, Copy)]
pub struct Colors {
    pub me: RwSignal<String>,
    pub party: RwSignal<String>,
    pub others: RwSignal<String>,
    pub summons: RwSignal<String>,
}

impl Colors {
    pub fn load() -> Self {
        let [me, party, others, summons] =
            DEFAULT_COLORS.map(|(key, default)| color_pref(key, default));
        Self {
            me,
            party,
            others,
            summons,
        }
    }

    fn all(&self) -> [RwSignal<String>; 4] {
        [self.me, self.party, self.others, self.summons]
    }

    pub fn reset(&self) {
        for (color, (_, default)) in self.all().iter().zip(DEFAULT_COLORS) {
            color.set(default.to_string());
        }
    }

    pub fn css_vars(&self) -> String {
        format!(
            "--c-me:{};--c-party:{};--c-others:{};--c-summons:{};",
            self.me.get(),
            self.party.get(),
            self.others.get(),
            self.summons.get()
        )
    }
}

pub struct FilteredRows {
    pub rows: Vec<ActorRow>,
    pub hidden: usize,
}

#[derive(Clone, Copy)]
pub struct MeterPrefs {
    pub bg_alpha: RwSignal<f64>,
    pub text_alpha: RwSignal<f64>,
    pub rows_in_view: RwSignal<u32>,
    pub show_details: RwSignal<bool>,
    pub map_mode: RwSignal<bool>,
    pub idle_secs: RwSignal<u32>,
    pub dps_kind: RwSignal<DpsKind>,
    pub recent_secs: RwSignal<u32>,
    pub show_self: RwSignal<bool>,
    pub show_party: RwSignal<bool>,
    pub show_others: RwSignal<bool>,
    pub show_summons: RwSignal<bool>,
    pub named_only: RwSignal<bool>,
    pub max_rows: RwSignal<usize>,
}

impl MeterPrefs {
    pub fn load() -> Self {
        let dps_kind = load(DPS_KIND_KEY)
            .as_deref()
            .and_then(DpsKind::parse)
            .unwrap_or(DEFAULT_DPS_KIND);
        Self {
            bg_alpha: alpha_pref(BG_ALPHA_KEY, DEFAULT_BG_ALPHA),
            text_alpha: alpha_pref(TEXT_ALPHA_KEY, DEFAULT_TEXT_ALPHA),
            rows_in_view: number_pref(ROWS_IN_VIEW_KEY, DEFAULT_ROWS_IN_VIEW, |rows| {
                ROWS_IN_VIEW_CHOICES.contains(rows)
            }),
            show_details: bool_pref(SHOW_DETAILS_KEY, DEFAULT_SHOW_DETAILS),
            map_mode: bool_pref(MAP_MODE_KEY, DEFAULT_MAP_MODE),
            idle_secs: number_pref(IDLE_SECS_KEY, DEFAULT_IDLE_SECS, |secs| {
                (MIN_IDLE_SECS..=MAX_IDLE_SECS).contains(secs)
            }),
            dps_kind: persisted(DPS_KIND_KEY, dps_kind, |kind| kind.key().to_string()),
            recent_secs: number_pref(RECENT_SECS_KEY, DEFAULT_RECENT_SECS, |secs| {
                (MIN_RECENT_SECS..=MAX_RECENT_SECS).contains(secs)
            }),
            show_self: bool_pref(SHOW_SELF_KEY, DEFAULT_SHOW_GROUP),
            show_party: bool_pref(SHOW_PARTY_KEY, DEFAULT_SHOW_GROUP),
            show_others: bool_pref(SHOW_OTHERS_KEY, DEFAULT_SHOW_GROUP),
            show_summons: bool_pref(SHOW_SUMMONS_KEY, DEFAULT_SHOW_GROUP),
            named_only: bool_pref(NAMED_ONLY_KEY, DEFAULT_NAMED_ONLY),
            max_rows: number_pref(MAX_ROWS_KEY, ALL_ROWS, |rows| {
                MAX_ROWS_CHOICES.contains(rows)
            }),
        }
    }

    pub fn reset_display(&self) {
        self.bg_alpha.set(DEFAULT_BG_ALPHA);
        self.text_alpha.set(DEFAULT_TEXT_ALPHA);
        self.rows_in_view.set(DEFAULT_ROWS_IN_VIEW);
        self.show_details.set(DEFAULT_SHOW_DETAILS);
    }

    pub fn reset_counting(&self) {
        self.map_mode.set(DEFAULT_MAP_MODE);
        self.idle_secs.set(DEFAULT_IDLE_SECS);
        self.dps_kind.set(DEFAULT_DPS_KIND);
        self.recent_secs.set(DEFAULT_RECENT_SECS);
    }

    pub fn reset_filters(&self) {
        self.show_self.set(DEFAULT_SHOW_GROUP);
        self.show_party.set(DEFAULT_SHOW_GROUP);
        self.show_others.set(DEFAULT_SHOW_GROUP);
        self.show_summons.set(DEFAULT_SHOW_GROUP);
        self.named_only.set(DEFAULT_NAMED_ONLY);
        self.max_rows.set(ALL_ROWS);
    }

    pub fn css_vars(&self) -> String {
        format!(
            "--bg-alpha:{:.2};--text-alpha:{:.2};--rows-n:{};",
            self.bg_alpha.get(),
            self.text_alpha.get(),
            self.rows_in_view.get()
        )
    }

    fn shows(&self, row: &ActorRow) -> bool {
        if row.is_summons {
            return self.show_summons.get();
        }
        if self.named_only.get() && row.name.is_none() && !row.is_self {
            return false;
        }
        if row.is_self {
            self.show_self.get()
        } else if row.is_party {
            self.show_party.get()
        } else {
            self.show_others.get()
        }
    }

    pub fn shown_damage(&self, rows: &[ActorRow]) -> u64 {
        rows.iter()
            .filter(|row| self.shows(row))
            .map(|row| row.damage)
            .sum()
    }

    pub fn filter_rows(&self, rows: Vec<ActorRow>) -> FilteredRows {
        let total = rows.len();
        let mut shown: Vec<ActorRow> = rows.into_iter().filter(|row| self.shows(row)).collect();
        let shown_damage = shown.iter().map(|row| row.damage).sum::<u64>().max(1);
        for row in &mut shown {
            row.share = row.damage as f64 / shown_damage as f64;
        }
        let max_rows = self.max_rows.get();
        if max_rows != ALL_ROWS {
            shown.truncate(max_rows);
        }
        FilteredRows {
            hidden: total - shown.len(),
            rows: shown,
        }
    }
}
