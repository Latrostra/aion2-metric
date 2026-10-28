use leptos::prelude::*;
use leptos::task::spawn_local;
use web_sys::KeyboardEvent;

use crate::i18n::tr;
use crate::tauri;

pub const DEFAULT_LOCK_SHORTCUT: &str = "Ctrl+Shift+KeyL";
const MODIFIER_CODE_PREFIXES: [&str; 5] = ["Control", "Shift", "Alt", "Meta", "OS"];
const KEY_CODE_PREFIXES: [&str; 2] = ["Key", "Digit"];
const CANCEL_CODE: &str = "Escape";

fn accel_from_event(event: &KeyboardEvent) -> Option<String> {
    let code = event.code();
    let modifier_only = code.is_empty()
        || MODIFIER_CODE_PREFIXES
            .iter()
            .any(|prefix| code.starts_with(prefix));
    if modifier_only {
        return None;
    }
    let modifiers: Vec<&str> = [
        (event.ctrl_key(), "Ctrl"),
        (event.alt_key(), "Alt"),
        (event.shift_key(), "Shift"),
        (event.meta_key(), "Super"),
    ]
    .into_iter()
    .filter_map(|(pressed, name)| pressed.then_some(name))
    .collect();
    if modifiers.is_empty() {
        return None;
    }
    Some(format!("{}+{code}", modifiers.join("+")))
}

fn display_key(part: &str) -> &str {
    if part == "Super" {
        return "Win";
    }
    KEY_CODE_PREFIXES
        .iter()
        .find_map(|prefix| part.strip_prefix(prefix))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(part)
}

pub fn display_accel(accel: &str) -> String {
    accel
        .split('+')
        .map(display_key)
        .collect::<Vec<_>>()
        .join("+")
}

pub async fn save_lock_shortcut(accel: String, shortcut: RwSignal<String>) -> Result<(), String> {
    let saved = tauri::set_lock_shortcut(&accel).await?;
    shortcut.set(saved.unwrap_or(accel));
    Ok(())
}

#[component]
pub fn ShortcutSetting(shortcut: RwSignal<String>) -> impl IntoView {
    let recording = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let listener = window_event_listener(leptos::ev::keydown, move |event: KeyboardEvent| {
        if !recording.get_untracked() {
            return;
        }
        event.prevent_default();
        if event.code() == CANCEL_CODE {
            recording.set(false);
            return;
        }
        let Some(accel) = accel_from_event(&event) else {
            return;
        };
        recording.set(false);
        spawn_local(async move {
            error.set(save_lock_shortcut(accel, shortcut).await.err());
        });
    });
    on_cleanup(move || listener.remove());
    view! {
        <div class="srow">
            <span class="slabel">{tr("Lock shortcut", "Skrót blokady")}</span>
            <button
                class="key"
                class:recording=move || recording.get()
                on:click=move |_| {
                    error.set(None);
                    recording.set(true);
                }
            >
                {move || {
                    if recording.get() {
                        tr("press keys… (Esc)", "naciśnij skrót… (Esc)").to_string()
                    } else {
                        display_accel(&shortcut.get())
                    }
                }}
            </button>
        </div>
        {move || error.get().map(|message| view! { <div class="settings-error">{message}</div> })}
    }
}
