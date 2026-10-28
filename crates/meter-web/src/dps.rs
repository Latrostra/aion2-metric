use leptos::prelude::*;
use meter_proto::{ActorRow, EncounterView};

use crate::i18n::tr;
use crate::prefs::{FilteredRows, MeterPrefs};

const PENDING_NAME: &str = "…";
const COMPACT_FROM: u64 = 10_000;
const THOUSAND: u64 = 1_000;
const MILLION: u64 = 1_000_000;
const HUNDREDTHS: u64 = 100;
const MS_PER_SECOND: i64 = 1_000;
const SECONDS_PER_MINUTE: i64 = 60;
const DETAIL_SEPARATOR: &str = " · ";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DpsKind {
    Average,
    Rolling,
    Active,
}

impl DpsKind {
    pub const ALL: [DpsKind; 3] = [DpsKind::Average, DpsKind::Rolling, DpsKind::Active];

    pub fn key(self) -> &'static str {
        match self {
            DpsKind::Average => "encounter",
            DpsKind::Rolling => "recent",
            DpsKind::Active => "active",
        }
    }

    pub fn parse(key: &str) -> Option<DpsKind> {
        DpsKind::ALL.into_iter().find(|kind| kind.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            DpsKind::Average => tr("Average", "Średni"),
            DpsKind::Rolling => tr("Rolling", "Kroczący"),
            DpsKind::Active => tr("Active", "Aktywny"),
        }
    }

    fn abbreviation(self) -> &'static str {
        match self {
            DpsKind::Average => tr("avg", "śr."),
            DpsKind::Rolling => tr("roll.", "krocz."),
            DpsKind::Active => tr("act.", "akt."),
        }
    }

    fn value(self, row: &ActorRow) -> f64 {
        match self {
            DpsKind::Average => row.dps,
            DpsKind::Rolling => row.dps_recent,
            DpsKind::Active => row.dps_active,
        }
    }

    fn summary(self, row: &ActorRow) -> String {
        format!(
            "{} {}/s",
            self.abbreviation(),
            format_number(self.value(row) as u64)
        )
    }
}

fn with_two_decimals(n: u64, unit: u64, suffix: char) -> String {
    format!("{}.{:02}{suffix}", n / unit, n % unit / (unit / HUNDREDTHS))
}

pub fn format_number(n: u64) -> String {
    if n < COMPACT_FROM {
        n.to_string()
    } else if n < MILLION {
        with_two_decimals(n, THOUSAND, 'k')
    } else {
        with_two_decimals(n, MILLION, 'M')
    }
}

pub fn format_duration(ms: i64) -> String {
    let secs = ms / MS_PER_SECOND;
    format!(
        "{}:{:02}",
        secs / SECONDS_PER_MINUTE,
        secs % SECONDS_PER_MINUTE
    )
}

#[component]
pub fn EncounterSummary(encounter: EncounterView, damage: u64, map_mode: bool) -> impl IntoView {
    let state = match (map_mode, encounter.active) {
        (true, true) => tr("map · fighting", "mapa · walka"),
        (true, false) => tr("map · idle", "mapa · przerwa"),
        (false, true) => tr("fighting", "walka"),
        (false, false) => tr("ended", "koniec"),
    };
    view! {
        <div class="enc-head">
            <span class:live=encounter.active>{state}</span>
            <span title=tr("combat time", "czas walki")>{format_duration(encounter.duration_ms)}</span>
            <span>{format_number(damage)} " dmg"</span>
        </div>
    }
}

#[component]
pub fn DpsRows(encounter: EncounterView, prefs: MeterPrefs) -> impl IntoView {
    let FilteredRows { rows, hidden } = prefs.filter_rows(encounter.rows);
    let top_damage = rows.first().map_or(1, |row| row.damage).max(1);
    let show_details = prefs.show_details.get();
    let main_kind = prefs.dps_kind.get();
    view! {
        <ul class="rows" class:compact=!show_details>
            {rows
                .into_iter()
                .map(|row| {
                    view! { <DpsRow row=row top_damage=top_damage show_details=show_details main_kind=main_kind /> }
                })
                .collect_view()}
        </ul>
        {(hidden > 0).then(|| view! { <p class="hidden-note">{format!("+{hidden} {}", tr("hidden", "ukrytych"))}</p> })}
    }
}

fn dps_tooltip(row: &ActorRow) -> String {
    let all_kinds: Vec<String> = DpsKind::ALL
        .into_iter()
        .map(|kind| kind.summary(row))
        .collect();
    format!(
        "{} ({})",
        all_kinds.join(DETAIL_SEPARATOR),
        format_duration(row.active_ms)
    )
}

fn details_line(row: &ActorRow, main_kind: DpsKind) -> String {
    let other_kinds: Vec<String> = DpsKind::ALL
        .into_iter()
        .filter(|&kind| kind != main_kind)
        .map(|kind| kind.summary(row))
        .collect();
    let mut details = format!(
        "{} · {} {} · {:.0}% crit · max {}",
        other_kinds.join(DETAIL_SEPARATOR),
        row.hits,
        tr("hits", "hit"),
        row.crit_rate * 100.0,
        format_number(u64::from(row.max_hit))
    );
    if row.summon_damage > 0 {
        details.push_str(&format!(
            " · {} {}",
            tr("pets", "pety"),
            format_number(row.summon_damage)
        ));
    }
    details
}

#[component]
fn DpsRow(row: ActorRow, top_damage: u64, show_details: bool, main_kind: DpsKind) -> impl IntoView {
    let bar_style = format!(
        "width:{:.1}%",
        row.damage as f64 / top_damage as f64 * 100.0
    );
    let name_pending = row.name.is_none();
    let name = row.name.clone().unwrap_or_else(|| PENDING_NAME.to_string());
    let name_title = if name_pending {
        tr("name not received yet", "nick jeszcze nie dotarł")
    } else {
        ""
    };
    let dps_title = dps_tooltip(&row);
    let details = details_line(&row, main_kind);
    view! {
        <li class="row" class:me=row.is_self class:party=row.is_party class:summons=row.is_summons>
            <div class="bar" style=bar_style></div>
            <div class="cell">
                <div class="line">
                    <span class="name" class:pending=name_pending title=name_title>{name}</span>
                    <span class="dps" title=dps_title>{format_number(main_kind.value(&row) as u64)} "/s"</span>
                    <span class="dmg">{format_number(row.damage)}</span>
                    <span class="share">{format!("{:.0}%", row.share * 100.0)}</span>
                </div>
                {show_details.then(|| view! { <div class="extra">{details}</div> })}
            </div>
        </li>
    }
}
