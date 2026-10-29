use leptos::prelude::*;
use meter_proto::IncomeView;

use crate::i18n::tr;
use crate::limits::group_digits;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 3_600;

fn income_entry(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="pet">
            <span class="pet-name">{label}</span>
            <span class="pet-count">{value}</span>
        </div>
    }
}

fn duration_text(total_secs: u64) -> String {
    let hours = total_secs / SECONDS_PER_HOUR;
    let minutes = total_secs % SECONDS_PER_HOUR / SECONDS_PER_MINUTE;
    let seconds = total_secs % SECONDS_PER_MINUTE;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[component]
pub fn IncomePanel(income: Signal<IncomeView>) -> impl IntoView {
    let waiting_banner = move || {
        (!income.get().kina.counting).then(|| {
            view! {
                <p class="pets-warn">
                    {tr(
                        "Counting starts after the first kina change or a relog.",
                        "Liczenie zacznie się po pierwszej zmianie kiny albo przelogowaniu.",
                    )}
                </p>
            }
        })
    };
    view! {
        <div class="pets">
            {waiting_banner}
            {move || {
                let income = income.get();
                view! {
                    {income_entry(tr("Time", "Czas"), duration_text(income.elapsed_secs))}
                    {income_entry(tr("Earned kina", "Zarobiona kina"), group_digits(income.kina.earned))}
                    {income_entry(tr("Kina/h", "Kina/h"), group_digits(income.kina.per_hour))}
                    {income_entry(tr("Abyss/h", "Abyss/h"), group_digits(income.abyss.per_hour))}
                }
            }}
        </div>
    }
}
