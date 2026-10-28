use leptos::prelude::*;
use meter_proto::{LimitView, LimitsView};

use crate::i18n::{Lang, lang, tr};

const PERCENT: f64 = 100.0;
const DIGIT_GROUP: usize = 3;
const UNKNOWN_VALUE: &str = "?";

pub fn group_digits(value: u64) -> String {
    let separator = match lang() {
        Lang::En => ',',
        Lang::Pl => '\u{a0}',
    };
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() * 4 / DIGIT_GROUP);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(DIGIT_GROUP) {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    grouped
}

fn limit_entry(label: &'static str, view: Option<LimitView>) -> impl IntoView {
    let (percent, count, reached) = match view {
        Some(LimitView { current, limit }) if limit > 0 => (
            (current as f64 / limit as f64).min(1.0) * PERCENT,
            format!("{} / {}", group_digits(current), group_digits(limit)),
            current >= limit,
        ),
        _ => (0.0, UNKNOWN_VALUE.to_string(), false),
    };
    view! {
        <div class="pet" class:done=reached>
            <div class="pet-bar" style=format!("width:{percent:.1}%")></div>
            <span class="pet-name">{label}</span>
            <span class="pet-count">{count}</span>
        </div>
    }
}

#[component]
pub fn LimitsPanel(limits: Signal<LimitsView>) -> impl IntoView {
    let relog_banner = move || {
        let limits = limits.get();
        (limits.kinah.is_none() || limits.abyss.is_none() || limits.gathering.is_none()).then(|| {
            view! {
                <p class="pets-warn">
                    {tr("Missing values appear after the next gain or a relog.", "Brakujące wartości pojawią się po kolejnym zdobyciu albo przelogowaniu.")}
                </p>
            }
        })
    };
    view! {
        <div class="pets">
            {relog_banner}
            {move || {
                let limits = limits.get();
                view! {
                    {limit_entry(tr("Kina today", "Kina dziś"), limits.kinah)}
                    {limit_entry(tr("Abyss points", "Punkty otchłani"), limits.abyss)}
                    {limit_entry(tr("Gathering today", "Zbieranie dziś"), limits.gathering)}
                }
            }}
        </div>
    }
}
