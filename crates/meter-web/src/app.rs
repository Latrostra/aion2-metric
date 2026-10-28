use std::time::Duration;

use leptos::html::Div;
use leptos::prelude::*;
use leptos::task::spawn_local;
use meter_proto::Snapshot;
use wasm_bindgen::JsCast;

use crate::dps::{DpsRows, EncounterSummary};
use crate::i18n::tr;
use crate::kinah::KinahPanel;
use crate::limits::LimitsPanel;
use crate::meter;
use crate::overlay::{
    LOCKED_CLASS, ResizeHandles, add_body_class, on_window_event, remember_unlocked_rows_height,
    scroll_by_wheel,
};
use crate::panels::{
    Panel, TabBar, expose_activate, expose_detached_setter, own_panel, report_tab_zone,
};
use crate::pets::PetsPanel;
use crate::prefs::{
    self, ACTIVE_PANEL_KEY, Colors, DEFAULT_HIDE_UNFOCUSED, HIDE_UNFOCUSED_KEY, MeterPrefs,
    PINNED_PETS_KEY, bool_pref, load_pinned_pets, pinned_pets_pref,
};
use crate::settings::Settings;
use crate::shortcut::{DEFAULT_LOCK_SHORTCUT, display_accel};
use crate::tauri;

const PANEL_RELOAD_DELAY: Duration = Duration::from_millis(400);
const TAB_ZONE_REPORT_DELAY: Duration = Duration::from_millis(50);

fn active_panel_pref() -> RwSignal<Panel> {
    let initial = prefs::load(ACTIVE_PANEL_KEY)
        .as_deref()
        .and_then(Panel::parse)
        .unwrap_or(Panel::Dps);
    prefs::persisted(ACTIVE_PANEL_KEY, initial, |panel| panel.key().to_string())
}

fn follow_other_windows(own: Option<Panel>, pinned: RwSignal<Vec<u32>>) {
    let reload_pending = StoredValue::new(false);
    on_window_event("storage", move |event| {
        let key = event
            .dyn_ref::<web_sys::StorageEvent>()
            .and_then(|storage_event| storage_event.key())
            .unwrap_or_default();
        if key == PINNED_PETS_KEY {
            pinned.set(load_pinned_pets());
        } else if own.is_some() && !reload_pending.get_value() {
            reload_pending.set_value(true);
            set_timeout(
                || {
                    let _ = window().location().reload();
                },
                PANEL_RELOAD_DELAY,
            );
        }
    });
}

fn load_overlay_state(
    own: Option<Panel>,
    lock_shortcut: RwSignal<String>,
    detached: RwSignal<Vec<String>>,
) {
    spawn_local(async move {
        if let Some(shortcut) = tauri::lock_shortcut().await {
            lock_shortcut.set(shortcut);
        }
        if tauri::is_locked().await {
            add_body_class(LOCKED_CLASS);
        }
        if own.is_none()
            && let Some(panels) = tauri::detached_panels().await
        {
            detached.set(panels);
        }
    });
}

#[component]
fn LockControls(lock_shortcut: RwSignal<String>) -> impl IntoView {
    view! {
        <button class="icon lock" title=tr("Lock", "Zablokuj") on:click=move |_| tauri::lock_overlay()>
            <svg
                viewBox="0 0 16 16"
                width="13"
                height="13"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
            >
                <rect x="3" y="7" width="10" height="7" rx="1.5"></rect>
                <path d="M5.5 7V4.5a2.5 2.5 0 0 1 5 0"></path>
            </svg>
        </button>
        <span class="hint">
            {move || format!("{} {}", display_accel(&lock_shortcut.get()), tr("to lock", "blokuje"))}
        </span>
    }
}

#[component]
fn Header(
    own: Option<Panel>,
    lock_shortcut: RwSignal<String>,
    show_settings: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <header>
            {own.is_none().then(|| view! { <LockControls lock_shortcut=lock_shortcut /> })}
            {own.map(|panel| view! { <span class="title">{panel.label()}</span> })}
            <span class="spacer"></span>
            <button class="icon restart" title=tr("Reset", "Resetuj") on:click=move |_| meter::reset_meter()>
                "↻"
            </button>
            {move || match own {
                Some(panel) => {
                    view! {
                        <button
                            class="icon attach"
                            title=tr("Back to tabs", "Wróć do zakładek")
                            on:click=move |_| tauri::attach_panel(panel.key())
                        >
                            "⤓"
                        </button>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        <button
                            class="icon gear"
                            title=tr("Settings", "Ustawienia")
                            class:open=move || show_settings.get()
                            on:click=move |_| show_settings.update(|open| *open = !*open)
                        >
                            "⚙"
                        </button>
                    }
                        .into_any()
                }
            }}
        </header>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let (snapshot, set_snapshot) = signal(None::<Snapshot>);
    let (online, set_online) = signal(false);
    let show_settings = RwSignal::new(false);
    let own = own_panel();
    let active = active_panel_pref();
    let detached = RwSignal::new(Vec::<String>::new());
    let pinned = pinned_pets_pref();
    let tabs = Memo::new(move |_| {
        if own.is_some() {
            return Vec::new();
        }
        let detached = detached.get();
        Panel::ALL
            .into_iter()
            .filter(|panel| !detached.iter().any(|key| key == panel.key()))
            .collect::<Vec<_>>()
    });
    let current = Memo::new(move |_| {
        own.or_else(|| {
            let tabs = tabs.get();
            let active = active.get();
            tabs.contains(&active)
                .then_some(active)
                .or(tabs.first().copied())
        })
    });
    follow_other_windows(own, pinned);
    let prefs = MeterPrefs::load();
    let colors = Colors::load();
    let hide_unfocused = bool_pref(HIDE_UNFOCUSED_KEY, DEFAULT_HIDE_UNFOCUSED);
    let lock_shortcut = RwSignal::new(DEFAULT_LOCK_SHORTCUT.to_string());
    load_overlay_state(own, lock_shortcut, detached);
    if own.is_none() {
        Effect::new(move |_| tauri::set_hide_unfocused(hide_unfocused.get()));
        expose_detached_setter(detached);
        expose_activate(active);
        on_window_event("resize", |_| report_tab_zone());
        Effect::new(move |_| {
            tabs.track();
            set_timeout(report_tab_zone, TAB_ZONE_REPORT_DELAY);
        });
    }
    let scroll_ref = NodeRef::<Div>::new();
    let sheet_ref = NodeRef::<Div>::new();
    Effect::new(move |_| {
        if let Some(rows) = scroll_ref.get() {
            remember_unlocked_rows_height(rows.into());
        }
    });
    let encounter = Memo::new(move |_| {
        let map_mode = prefs.map_mode.get();
        snapshot.get().and_then(|snapshot| {
            if map_mode {
                snapshot.session
            } else {
                snapshot.encounter
            }
        })
    });
    Effect::new(move |_| {
        if online.get() {
            meter::send_idle_secs(prefs.idle_secs.get());
        }
    });
    Effect::new(move |_| {
        if online.get() {
            meter::send_recent_secs(prefs.recent_secs.get());
        }
    });
    Effect::new(move |_| {
        if online.get() {
            meter::send_pinned_pets(&pinned.get());
        }
    });
    meter::connect(set_snapshot, set_online);

    let style_vars = move || format!("{}{}", prefs.css_vars(), colors.css_vars());

    let on_wheel = move |event: web_sys::WheelEvent| {
        let scrolled = if show_settings.get_untracked() {
            sheet_ref.get_untracked()
        } else {
            scroll_ref.get_untracked()
        };
        scroll_by_wheel(scrolled, &event);
    };

    view! {
        <main class="meter" data-tauri-drag-region="true" style=style_vars on:wheel=on_wheel>
            <ResizeHandles />
            <Header own=own lock_shortcut=lock_shortcut show_settings=show_settings />
            <Show when=move || !tabs.get().is_empty()>
                <TabBar tabs=tabs current=current active=active />
            </Show>
            <div class="body">
                {move || match (current.get(), encounter.get()) {
                    (None, _) => {
                        view! { <p class="idle">{tr("All panels are detached.", "Wszystkie panele są odpięte.")}</p> }
                            .into_any()
                    }
                    (Some(Panel::Pets | Panel::Limits | Panel::Kinah), _) => ().into_any(),
                    (Some(Panel::Dps), None) => {
                        view! {
                            <p class="idle">
                                {move || {
                                    if online.get() {
                                        tr("Waiting for combat…", "Czekam na walkę…")
                                    } else {
                                        tr("Connecting…", "Łączenie…")
                                    }
                                }}
                            </p>
                        }
                            .into_any()
                    }
                    (Some(Panel::Dps), Some(encounter)) => {
                        let damage = prefs.shown_damage(&encounter.rows);
                        view! {
                            <EncounterSummary
                                encounter=encounter
                                damage=damage
                                map_mode=prefs.map_mode.get_untracked()
                            />
                        }
                            .into_any()
                    }
                }}
                <div class="scroll" node_ref=scroll_ref>
                    {move || {
                        (current.get() == Some(Panel::Dps))
                            .then(|| encounter.get().map(|encounter| view! { <DpsRows encounter=encounter prefs=prefs /> }))
                    }}
                    <Show when=move || current.get() == Some(Panel::Pets)>
                        <PetsPanel
                            pets=Signal::derive(move || snapshot.get().map(|snapshot| snapshot.pets).unwrap_or_default())
                            loaded=Signal::derive(move || {
                                snapshot.get().is_some_and(|snapshot| snapshot.status.pet_collection)
                            })
                            pinned=pinned
                        />
                    </Show>
                    <Show when=move || current.get() == Some(Panel::Limits)>
                        <LimitsPanel limits=Signal::derive(move || {
                            snapshot.get().map(|snapshot| snapshot.limits).unwrap_or_default()
                        }) />
                    </Show>
                    <Show when=move || current.get() == Some(Panel::Kinah)>
                        <KinahPanel kinah=Signal::derive(move || {
                            snapshot.get().map(|snapshot| snapshot.kinah).unwrap_or_default()
                        }) />
                    </Show>
                </div>
                <Show when=move || show_settings.get()>
                    <div class="sheet" node_ref=sheet_ref>
                        <Settings
                            prefs=prefs
                            colors=colors
                            lock_shortcut=lock_shortcut
                            hide_unfocused=hide_unfocused
                            party_size=Signal::derive(move || {
                                snapshot.get().map_or(0, |snapshot| snapshot.status.party_size)
                            })
                        />
                    </div>
                </Show>
            </div>
        </main>
    }
}
