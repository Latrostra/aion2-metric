use meter_proto::{GainView, IncomeView};

const MS_PER_SECOND: i64 = 1_000;
const MS_PER_HOUR: i64 = 3_600_000;

#[derive(Debug, Default)]
pub struct GainTracker {
    balance: Option<u64>,
    earned: u64,
}

impl GainTracker {
    fn on_balance(&mut self, balance: u64) -> u64 {
        let gained = self
            .balance
            .map_or(0, |previous| balance.saturating_sub(previous));
        self.earned += gained;
        self.balance = Some(balance);
        gained
    }

    fn per_hour(&self, elapsed_ms: i64) -> u64 {
        let elapsed_ms = elapsed_ms.max(MS_PER_SECOND);
        let per_hour = u128::from(self.earned) * MS_PER_HOUR as u128 / elapsed_ms as u128;
        u64::try_from(per_hour).unwrap_or(u64::MAX)
    }

    fn view(&self, elapsed_ms: Option<i64>) -> GainView {
        GainView {
            counting: self.balance.is_some(),
            earned: self.earned,
            per_hour: elapsed_ms.map_or(0, |elapsed_ms| self.per_hour(elapsed_ms)),
        }
    }
}

#[derive(Debug, Default)]
pub struct Income {
    kina: GainTracker,
    abyss: GainTracker,
    first_gain_ms: Option<i64>,
    now_ms: i64,
}

impl Income {
    pub fn tick(&mut self, ts_ms: i64) {
        self.now_ms = self.now_ms.max(ts_ms);
    }

    pub fn on_kina_balance(&mut self, ts_ms: i64, balance: u64) {
        let gained = self.kina.on_balance(balance);
        self.start_clock_on_gain(ts_ms, gained);
    }

    pub fn on_abyss_points(&mut self, ts_ms: i64, points: u64) {
        let gained = self.abyss.on_balance(points);
        self.start_clock_on_gain(ts_ms, gained);
    }

    fn start_clock_on_gain(&mut self, ts_ms: i64, gained: u64) {
        if gained > 0 && self.first_gain_ms.is_none() {
            self.first_gain_ms = Some(ts_ms);
        }
    }

    pub fn reset(&mut self) {
        self.kina.earned = 0;
        self.abyss.earned = 0;
        self.first_gain_ms = None;
    }

    pub fn view(&self) -> IncomeView {
        let elapsed_ms = self
            .first_gain_ms
            .map(|first_gain_ms| (self.now_ms - first_gain_ms).max(0));
        IncomeView {
            elapsed_secs: elapsed_ms.map_or(0, |elapsed_ms| (elapsed_ms / MS_PER_SECOND) as u64),
            kina: self.kina.view(elapsed_ms),
            abyss: self.abyss.view(elapsed_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE_MS: i64 = 60_000;

    #[test]
    fn earned_counts_only_gains() {
        let mut income = Income::default();
        for balance in [1_000, 1_415, 1_200, 1_700] {
            income.on_kina_balance(0, balance);
        }
        assert_eq!(income.view().kina.earned, 415 + 500);
    }

    #[test]
    fn clock_starts_at_the_first_gain() {
        let mut income = Income::default();
        income.on_kina_balance(0, 1_000);
        income.tick(5 * MINUTE_MS);
        assert_eq!(income.view().elapsed_secs, 0);
        assert_eq!(income.view().kina.per_hour, 0);
        income.on_kina_balance(5 * MINUTE_MS, 1_500);
        income.tick(5 * MINUTE_MS);
        assert_eq!(income.view().kina.per_hour, 1_800_000);
        income.tick(15 * MINUTE_MS);
        let view = income.view();
        assert_eq!(view.elapsed_secs, 600);
        assert_eq!(view.kina.per_hour, 3_000);
    }

    #[test]
    fn abyss_gain_also_starts_the_clock() {
        let mut income = Income::default();
        income.on_abyss_points(0, 39_000);
        income.on_abyss_points(MINUTE_MS, 39_100);
        income.on_kina_balance(MINUTE_MS, 5_000);
        income.tick(61 * MINUTE_MS);
        let view = income.view();
        assert_eq!(view.elapsed_secs, 3_600);
        assert_eq!((view.abyss.per_hour, view.kina.per_hour), (100, 0));
    }

    #[test]
    fn reset_waits_for_the_next_gain() {
        let mut income = Income::default();
        income.on_kina_balance(0, 1_000);
        income.on_kina_balance(MINUTE_MS, 2_000);
        income.tick(5 * MINUTE_MS);
        income.reset();
        let view = income.view();
        assert_eq!(
            (view.kina.earned, view.kina.per_hour, view.elapsed_secs),
            (0, 0, 0)
        );
        assert!(view.kina.counting);
        income.on_kina_balance(6 * MINUTE_MS, 2_400);
        income.tick(66 * MINUTE_MS);
        let view = income.view();
        assert_eq!((view.kina.per_hour, view.elapsed_secs), (400, 3_600));
    }
}
