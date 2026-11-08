use std::time::Duration;

/// 指数退避：从 `min` 开始翻倍，封顶 `max`；可选对称抖动，避免多个进程同时重连。
#[derive(Debug, Clone)]
pub struct Backoff {
    min: Duration,
    max: Duration,
    next: Duration,
    jitter_ratio: f64,
}

impl Backoff {
    pub fn new(min: Duration, max: Duration) -> Self {
        Self::with_jitter(min, max, 0.0)
    }

    /// 与旧 adapter 配置语义一致：最小 1 秒，最大值不得小于最小值。
    pub fn from_secs(min_secs: u64, max_secs: u64) -> Self {
        Self::from_secs_jittered(min_secs, max_secs, 0.0)
    }

    pub fn from_secs_jittered(min_secs: u64, max_secs: u64, jitter_ratio: f64) -> Self {
        let min = min_secs.max(1);
        let max = max_secs.max(min);
        Self::with_jitter(
            Duration::from_secs(min),
            Duration::from_secs(max),
            jitter_ratio,
        )
    }

    pub fn with_jitter(min: Duration, max: Duration, jitter_ratio: f64) -> Self {
        let max = max.max(min);
        Self {
            min,
            max,
            next: min,
            jitter_ratio: jitter_ratio.clamp(0.0, 1.0),
        }
    }

    pub fn reset(&mut self) {
        self.next = self.min;
    }

    pub fn next_delay(&mut self) -> Duration {
        let base = self.next;
        self.next = self.next.saturating_mul(2).min(self.max);

        if self.jitter_ratio == 0.0 {
            return base;
        }

        let ratio = self.jitter_ratio;
        let low = base.as_secs_f64() * (1.0 - ratio);
        let high = base.as_secs_f64() * (1.0 + ratio);
        let sampled = low + rand::random::<f64>() * (high - low);
        Duration::from_secs_f64(
            sampled.clamp(self.min.as_secs_f64(), self.max.as_secs_f64()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_and_caps() {
        let mut b = Backoff::new(Duration::from_secs(1), Duration::from_secs(4));
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_secs(2));
        assert_eq!(b.next_delay(), Duration::from_secs(4));
        assert_eq!(b.next_delay(), Duration::from_secs(4));
        b.reset();
        assert_eq!(b.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn zero_config_clamps_to_one_second() {
        let mut b = Backoff::from_secs(0, 0);
        assert_eq!(b.next_delay(), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn jitter_stays_within_bounds_and_caps() {
        let mut b = Backoff::with_jitter(Duration::from_secs(4), Duration::from_secs(8), 0.5);
        for _ in 0..200 {
            let d = b.next_delay();
            assert!(d >= Duration::from_secs(4), "delay below min: {d:?}");
            assert!(d <= Duration::from_secs(8), "delay above max: {d:?}");
        }
    }

    #[test]
    fn reset_returns_to_min() {
        let mut b = Backoff::with_jitter(Duration::from_secs(2), Duration::from_secs(16), 0.3);
        let _ = b.next_delay();
        let _ = b.next_delay();
        b.reset();
        let d = b.next_delay();
        assert!(d >= Duration::from_secs(2) && d <= Duration::from_secs(16));
    }
}
