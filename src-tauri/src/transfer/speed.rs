use std::collections::VecDeque;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(8);
const MIN_ELAPSED: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub(super) struct SpeedSmoother {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
    bytes_per_sec: Option<u64>,
}

impl Default for SpeedSmoother {
    fn default() -> Self {
        Self::new(WINDOW)
    }
}

impl SpeedSmoother {
    pub(super) fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
            bytes_per_sec: None,
        }
    }

    pub(super) fn record(&mut self, at: Instant, total_bytes: u64) -> Option<u64> {
        while let Some((time, _)) = self.samples.front() {
            if at.duration_since(*time) <= self.window {
                break;
            }
            self.samples.pop_front();
        }
        if self
            .samples
            .back()
            .is_none_or(|(_, bytes)| *bytes != total_bytes)
        {
            self.samples.push_back((at, total_bytes));
        }
        self.bytes_per_sec = self.rate_at(at);
        self.bytes_per_sec
    }

    pub(super) fn current(&self) -> Option<u64> {
        self.bytes_per_sec
    }

    fn rate_at(&self, at: Instant) -> Option<u64> {
        let (old_time, old_bytes) = self.samples.front()?;
        let (_, latest_bytes) = self.samples.back()?;
        let elapsed = at.duration_since(*old_time);
        if elapsed < MIN_ELAPSED || latest_bytes <= old_bytes {
            return None;
        }
        Some((((*latest_bytes - *old_bytes) as f64) / elapsed.as_secs_f64()).round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_enough_samples() {
        let start = Instant::now();
        let mut smoother = SpeedSmoother::new(Duration::from_secs(8));
        assert_eq!(smoother.record(start, 0), None);
        assert_eq!(
            smoother.record(start + Duration::from_millis(200), 1_000),
            None
        );
    }

    #[test]
    fn reports_sliding_window_bytes_per_second() {
        let start = Instant::now();
        let mut smoother = SpeedSmoother::new(Duration::from_secs(5));
        smoother.record(start, 0);
        assert_eq!(
            smoother.record(start + Duration::from_secs(2), 20_000),
            Some(10_000)
        );
        assert_eq!(
            smoother.record(start + Duration::from_secs(6), 60_000),
            Some(10_000)
        );
    }
}
