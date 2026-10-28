use meter_proto::KinahView;

const MS_PER_HOUR: i64 = 3_600_000;
const MIN_RATE_WINDOW_MS: i64 = 60_000;

#[derive(Debug, Default)]
pub struct KinahTracker {
    balance: Option<u64>,
    earned: u64,
    counting_since_ms: Option<i64>,
    now_ms: i64,
}

impl KinahTracker {
    pub fn tick(&mut self, ts_ms: i64) {
        self.now_ms = self.now_ms.max(ts_ms);
    }

    pub fn on_balance(&mut self, ts_ms: i64, balance: u64) {
        match self.balance {
            Some(previous) => self.earned += balance.saturating_sub(previous),
            None => self.counting_since_ms = Some(ts_ms),
        }
        self.balance = Some(balance);
    }

    pub fn reset(&mut self) {
        self.earned = 0;
        self.counting_since_ms = self.balance.map(|_| self.now_ms);
    }

    fn per_hour(&self) -> Option<u64> {
        let elapsed_ms = self.now_ms - self.counting_since_ms?;
        (elapsed_ms >= MIN_RATE_WINDOW_MS).then(|| {
            let per_hour = u128::from(self.earned) * MS_PER_HOUR as u128 / elapsed_ms as u128;
            u64::try_from(per_hour).unwrap_or(u64::MAX)
        })
    }

    pub fn view(&self) -> KinahView {
        KinahView {
            counting: self.balance.is_some(),
            earned: self.earned,
            per_hour: self.per_hour(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE_MS: i64 = 60_000;

    #[test]
    fn earned_counts_only_gains() {
        let mut tracker = KinahTracker::default();
        for balance in [1_000, 1_415, 1_200, 1_700] {
            tracker.on_balance(0, balance);
        }
        assert_eq!(tracker.view().earned, 415 + 500);
    }

    #[test]
    fn rate_is_projected_to_an_hour_after_a_minute() {
        let mut tracker = KinahTracker::default();
        tracker.on_balance(0, 1_000);
        tracker.on_balance(MINUTE_MS / 2, 1_500);
        tracker.tick(MINUTE_MS / 2);
        assert_eq!(tracker.view().per_hour, None);
        tracker.tick(10 * MINUTE_MS);
        assert_eq!(tracker.view().per_hour, Some(3_000));
    }

    #[test]
    fn reset_starts_counting_from_now() {
        let mut tracker = KinahTracker::default();
        tracker.on_balance(0, 1_000);
        tracker.on_balance(MINUTE_MS, 2_000);
        tracker.tick(5 * MINUTE_MS);
        tracker.reset();
        let view = tracker.view();
        assert_eq!((view.earned, view.per_hour, view.counting), (0, None, true));
        tracker.on_balance(6 * MINUTE_MS, 2_400);
        tracker.tick(65 * MINUTE_MS);
        assert_eq!(tracker.view().per_hour, Some(400));
    }

    #[test]
    fn nothing_is_counted_before_the_first_balance() {
        let mut tracker = KinahTracker::default();
        tracker.tick(10 * MINUTE_MS);
        tracker.reset();
        let view = tracker.view();
        assert_eq!(
            (view.earned, view.per_hour, view.counting),
            (0, None, false)
        );
    }
}
