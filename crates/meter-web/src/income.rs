use leptos::prelude::*;
use meter_proto::IncomeView;

use crate::i18n::tr;
use crate::limits::group_digits;

const UNKNOWN_VALUE: &str = "?";

fn income_entry(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="pet">
            <span class="pet-name">{label}</span>
            <span class="pet-count">{value}</span>
        </div>
    }
}

fn rate_text(per_hour: Option<u64>) -> String {
    per_hour
        .map(group_digits)
        .unwrap_or_else(|| UNKNOWN_VALUE.to_string())
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
                    {income_entry(tr("Earned kina", "Zarobiona kina"), group_digits(income.kina.earned))}
                    {income_entry(tr("Kina/h", "Kina/h"), rate_text(income.kina.per_hour))}
                    {income_entry(tr("Abyss/h", "Abyss/h"), rate_text(income.abyss.per_hour))}
                }
            }}
        </div>
    }
}
