use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::controls::{
    color_swatch, percent_label, percent_of, range_u32, seconds_label, seg, srow, switch,
};
use crate::dps::DpsKind;
use crate::i18n::{self, Lang, tr};
use crate::prefs::{
    self, ALL_ROWS, Colors, LANG_KEY, MAX_IDLE_SECS, MAX_RECENT_SECS, MAX_ROWS_CHOICES,
    MIN_IDLE_SECS, MIN_RECENT_SECS, MeterPrefs, ROWS_IN_VIEW_CHOICES, SETTINGS_TAB_KEY,
};
use crate::shortcut::{DEFAULT_LOCK_SHORTCUT, ShortcutSetting, save_lock_shortcut};

const PARTY_LABEL: &str = "Party";
const MAX_PERCENT: u32 = 100;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Display,
    Dps,
    Filters,
    General,
}

impl SettingsTab {
    const ALL: [SettingsTab; 4] = [
        SettingsTab::Display,
        SettingsTab::Dps,
        SettingsTab::Filters,
        SettingsTab::General,
    ];

    fn key(self) -> &'static str {
        match self {
            SettingsTab::Display => "look",
            SettingsTab::Dps => "counting",
            SettingsTab::Filters => "filters",
            SettingsTab::General => "general",
        }
    }

    fn parse(key: &str) -> Option<SettingsTab> {
        SettingsTab::ALL.into_iter().find(|tab| tab.key() == key)
    }

    fn label(self) -> &'static str {
        match self {
            SettingsTab::Display => tr("Display", "Wygląd"),
            SettingsTab::Dps => "DPS",
            SettingsTab::Filters => tr("Filters", "Filtry"),
            SettingsTab::General => tr("General", "Ogólne"),
        }
    }
}

fn switch_language(lang: Lang) {
    if lang == i18n::lang() {
        return;
    }
    prefs::save(LANG_KEY, lang.key());
    let _ = window().location().reload();
}

fn labeled<T: Copy>(
    values: impl IntoIterator<Item = T>,
    label: impl Fn(T) -> String,
) -> Vec<(T, String)> {
    values
        .into_iter()
        .map(|value| (value, label(value)))
        .collect()
}

fn display_tab(
    prefs: MeterPrefs,
    colors: Colors,
    bg_percent: RwSignal<u32>,
    text_percent: RwSignal<u32>,
) -> impl IntoView {
    view! {
        {srow(tr("Background", "Tło"), range_u32(bg_percent, 0, MAX_PERCENT, percent_label))}
        {srow(tr("Text", "Tekst"), range_u32(text_percent, 0, MAX_PERCENT, percent_label))}
        {srow(
            tr("Rows", "Wiersze"),
            seg(labeled(ROWS_IN_VIEW_CHOICES, |rows| rows.to_string()), prefs.rows_in_view),
        )}
        {srow(tr("Details", "Szczegóły"), switch(prefs.show_details))}
        <div class="sgroup">{tr("Colors", "Kolory")}</div>
        <div class="swatches">
            {color_swatch(tr("You", "Ty"), colors.me)}
            {color_swatch(PARTY_LABEL, colors.party)}
            {color_swatch(tr("Others", "Inni"), colors.others)}
            {color_swatch(tr("Pets", "Pety"), colors.summons)}
        </div>
    }
}

fn dps_tab(prefs: MeterPrefs) -> impl IntoView {
    view! {
        {srow(
            tr("Mode", "Tryb"),
            seg(
                vec![
                    (true, tr("Whole map", "Cała mapa").to_string()),
                    (false, tr("Per fight", "Na walkę").to_string()),
                ],
                prefs.map_mode,
            ),
        )}
        <Show when=move || !prefs.map_mode.get()>
            {srow(
                tr("Fight ends after", "Koniec walki po"),
                range_u32(prefs.idle_secs, MIN_IDLE_SECS, MAX_IDLE_SECS, seconds_label),
            )}
        </Show>
        {srow(
            tr("Main value", "Główna liczba"),
            seg(labeled(DpsKind::ALL, |kind| kind.label().to_string()), prefs.dps_kind),
        )}
        {srow(
            tr("Rolling window", "Okno kroczące"),
            range_u32(prefs.recent_secs, MIN_RECENT_SECS, MAX_RECENT_SECS, seconds_label),
        )}
    }
}

fn max_rows_label(rows: usize) -> String {
    if rows == ALL_ROWS {
        tr("All", "Wsz.").to_string()
    } else {
        rows.to_string()
    }
}

fn filters_tab(prefs: MeterPrefs, party_size: Signal<u64>) -> impl IntoView {
    let party_label = move || match party_size.get() {
        0 => PARTY_LABEL.to_string(),
        size => format!("{PARTY_LABEL} ({size})"),
    };
    view! {
        {srow(tr("You", "Ty"), switch(prefs.show_self))}
        <div class="srow">
            <span class="slabel">{party_label}</span>
            {switch(prefs.show_party)}
        </div>
        {srow(tr("Other players", "Inni gracze"), switch(prefs.show_others))}
        {srow(tr("Unowned pets", "Obce pety"), switch(prefs.show_summons))}
        {srow(tr("Named only", "Tylko z nickiem"), switch(prefs.named_only))}
        {srow(tr("Max rows", "Maks. wierszy"), seg(labeled(MAX_ROWS_CHOICES, max_rows_label), prefs.max_rows))}
    }
}

fn general_tab(
    lang: RwSignal<Lang>,
    lock_shortcut: RwSignal<String>,
    hide_unfocused: RwSignal<bool>,
) -> impl IntoView {
    view! {
        {srow(tr("Language", "Język"), seg(labeled(Lang::ALL, |lang| lang.name().to_string()), lang))}
        <ShortcutSetting shortcut=lock_shortcut />
        {srow(tr("Hide outside game", "Chowaj poza grą"), switch(hide_unfocused))}
    }
}

#[component]
pub fn Settings(
    prefs: MeterPrefs,
    colors: Colors,
    lock_shortcut: RwSignal<String>,
    hide_unfocused: RwSignal<bool>,
    party_size: Signal<u64>,
) -> impl IntoView {
    let open_tab = prefs::persisted(
        SETTINGS_TAB_KEY,
        prefs::load(SETTINGS_TAB_KEY)
            .as_deref()
            .and_then(SettingsTab::parse)
            .unwrap_or(SettingsTab::Display),
        |tab| tab.key().to_string(),
    );
    let bg_percent = percent_of(prefs.bg_alpha);
    let text_percent = percent_of(prefs.text_alpha);
    let lang = RwSignal::new(i18n::lang());
    Effect::new(move |_| switch_language(lang.get()));

    let reset_open_tab = move |_| match open_tab.get_untracked() {
        SettingsTab::Display => {
            prefs.reset_display();
            colors.reset();
        }
        SettingsTab::Dps => prefs.reset_counting(),
        SettingsTab::Filters => prefs.reset_filters(),
        SettingsTab::General => {
            spawn_local(async move {
                let _ = save_lock_shortcut(DEFAULT_LOCK_SHORTCUT.to_string(), lock_shortcut).await;
            });
        }
    };

    view! {
        <div class="settings">
            <nav class="tabs">
                {SettingsTab::ALL
                    .into_iter()
                    .map(|tab| {
                        view! {
                            <button
                                class="tab"
                                class:active=move || open_tab.get() == tab
                                on:click=move |_| open_tab.set(tab)
                            >
                                {tab.label()}
                            </button>
                        }
                    })
                    .collect_view()}
            </nav>
            <div class="tab-body">
                {move || match open_tab.get() {
                    SettingsTab::Display => display_tab(prefs, colors, bg_percent, text_percent).into_any(),
                    SettingsTab::Dps => dps_tab(prefs).into_any(),
                    SettingsTab::Filters => filters_tab(prefs, party_size).into_any(),
                    SettingsTab::General => general_tab(lang, lock_shortcut, hide_unfocused).into_any(),
                }}
            </div>
            <div class="settings-foot">
                <button class="reset" on:click=reset_open_tab>
                    {tr("Reset tab", "Przywróć domyślne")}
                </button>
            </div>
        </div>
    }
}
