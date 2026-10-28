use leptos::prelude::*;

const PERCENT: f64 = 100.0;
const PERCENT_SYNC_TOLERANCE: f64 = 0.004;

pub fn srow(label: &'static str, control: impl IntoView + 'static) -> impl IntoView {
    view! {
        <div class="srow">
            <span class="slabel">{label}</span>
            {control}
        </div>
    }
}

pub fn seg<T: Copy + PartialEq + Send + Sync + 'static>(
    options: Vec<(T, String)>,
    value: RwSignal<T>,
) -> impl IntoView {
    view! {
        <div class="seg">
            {options
                .into_iter()
                .map(|(option, label)| {
                    view! {
                        <button class:on=move || value.get() == option on:click=move |_| value.set(option)>
                            {label}
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

pub fn switch(value: RwSignal<bool>) -> impl IntoView {
    view! {
        <button
            class="switch"
            class:on=move || value.get()
            role="switch"
            aria-checked=move || value.get().to_string()
            on:click=move |_| value.update(|on| *on = !*on)
        ></button>
    }
}

pub fn range_u32(
    value: RwSignal<u32>,
    min: u32,
    max: u32,
    format_value: fn(u32) -> String,
) -> impl IntoView {
    view! {
        <div class="range">
            <input
                type="range"
                min=min.to_string()
                max=max.to_string()
                step="1"
                prop:value=move || value.get().to_string()
                on:input=move |event| {
                    if let Ok(picked) = event_target_value(&event).parse::<u32>() {
                        value.set(picked.clamp(min, max));
                    }
                }
            />
            <span class="rvalue">{move || format_value(value.get())}</span>
        </div>
    }
}

pub fn color_swatch(label: &'static str, value: RwSignal<String>) -> impl IntoView {
    view! {
        <label class="swatch">
            <input
                type="color"
                prop:value=move || value.get()
                on:input=move |event| value.set(event_target_value(&event))
            />
            <span>{label}</span>
        </label>
    }
}

pub fn percent_label(percent: u32) -> String {
    format!("{percent}%")
}

pub fn seconds_label(secs: u32) -> String {
    format!("{secs} s")
}

fn to_percent(fraction: f64) -> u32 {
    (fraction * PERCENT).round() as u32
}

pub fn percent_of(fraction: RwSignal<f64>) -> RwSignal<u32> {
    let percent = RwSignal::new(to_percent(fraction.get_untracked()));
    Effect::new(move |_| {
        let current = to_percent(fraction.get());
        if percent.get_untracked() != current {
            percent.set(current);
        }
    });
    Effect::new(move |_| {
        let picked = f64::from(percent.get()) / PERCENT;
        if (fraction.get_untracked() - picked).abs() > PERCENT_SYNC_TOLERANCE {
            fraction.set(picked);
        }
    });
    percent
}
