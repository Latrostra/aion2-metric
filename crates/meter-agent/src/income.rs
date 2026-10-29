use meter_proto::{GainView, IncomeView};

const MS_PER_HOUR: i64 = 3_600_000;
const MIN_RATE_WINDOW_MS: i64 = 60_000;

#[derive(Debug, Default)]
pub struct GainTracker {
    balance: Option<u64>,
    earned: u64,
    counting_since_ms: Option<i64>,
}

impl GainTracker {
    pub fn on_balance(&mut self, ts_ms: i64, balance: u64) {
        match self.balance {
            Some(previous) => self.earned += balance.saturating_sub(previous),
            None => self.counting_since_ms = Some(ts_ms),
        }
        self.balance = Some(balance);
    }

    pub fn reset(&mut self, now_ms: i64) {
        self.earned = 0;
        self.counting_since_ms = self.balance.map(|_| now_ms);
    }

    fn per_hour(&self, now_ms: i64) -> Option<u64> {
        let elapsed_ms = now_ms - self.counting_since_ms?;
        (elapsed_ms >= MIN_RATE_WINDOW_MS).then(|| {
            let per_hour = u128::from(self.earned) * MS_PER_HOUR as u128 / elapsed_ms as u128;
            u64::try_from(per_hour).unwrap_or(u64::MAX)
        })
    }

    pub fn view(&self, now_ms: i64) -> GainView {
        GainView {
            counting: self.balance.is_some(),
            earned: self.earned,
            per_hour: self.per_hour(now_ms),
        }
    }
}

#[derive(Debug, Default)]
pub struct Income {
    pub kina: GainTracker,
    pub abyss: GainTracker,
    now_ms: i64,
}

impl Income {
    pub fn tick(&mut self, ts_ms: i64) {
        self.now_ms = self.now_ms.max(ts_ms);
    }

    pub fn reset(&mut self) {
        self.kina.reset(self.now_ms);
        self.abyss.reset(self.now_ms);
    }

    pub fn view(&self) -> IncomeView {
        IncomeView {
            kina: self.kina.view(self.now_ms),
            abyss: self.abyss.view(self.now_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE_MS: i64 = 60_000;

    #[test]
    fn earned_counts_only_gains() {
        let mut tracker = GainTracker::default();
        for balance in [1_000, 1_415, 1_200, 1_700] {
            tracker.on_balance(0, balance);
        }
        assert_eq!(tracker.view(0).earned, 415 + 500);
    }

    #[test]
    fn rate_is_projected_to_an_hour_after_a_minute() {
        let mut tracker = GainTracker::default();
        tracker.on_balance(0, 1_000);
        tracker.on_balance(MINUTE_MS / 2, 1_500);
        assert_eq!(tracker.view(MINUTE_MS / 2).per_hour, None);
        assert_eq!(tracker.view(10 * MINUTE_MS).per_hour, Some(3_000));
    }

    #[test]
    fn reset_starts_counting_from_now() {
        let mut income = Income::default();
        income.kina.on_balance(0, 1_000);
        income.kina.on_balance(MINUTE_MS, 2_000);
        income.abyss.on_balance(0, 39_000);
        income.abyss.on_balance(MINUTE_MS, 39_100);
        income.tick(5 * MINUTE_MS);
        income.reset();
        let view = income.view();
        assert_eq!(
            (view.kina.earned, view.kina.per_hour, view.kina.counting),
            (0, None, true)
        );
        assert_eq!(view.abyss.earned, 0);
        income.kina.on_balance(6 * MINUTE_MS, 2_400);
        income.abyss.on_balance(6 * MINUTE_MS, 39_160);
        income.tick(65 * MINUTE_MS);
        let view = income.view();
        assert_eq!(view.kina.per_hour, Some(400));
        assert_eq!(view.abyss.per_hour, Some(60));
    }

    #[test]
    fn nothing_is_counted_before_the_first_balance() {
        let mut income = Income::default();
        income.tick(10 * MINUTE_MS);
        income.reset();
        let view = income.view().kina;
        assert_eq!(
            (view.earned, view.per_hour, view.counting),
            (0, None, false)
        );
    }
}
