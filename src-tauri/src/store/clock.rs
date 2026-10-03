use parking_lot::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Hybrid logical clock timestamp, serialized so that string order == causal order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hlc {
    pub wall_ms: u64,
    pub counter: u32,
    pub node: String,
}

impl Hlc {
    pub fn encode(&self) -> String {
        format!("{:015}-{:08}-{}", self.wall_ms, self.counter, self.node)
    }

    pub fn decode(value: &str) -> Option<Self> {
        let mut parts = value.splitn(3, '-');
        Some(Self {
            wall_ms: parts.next()?.parse().ok()?,
            counter: parts.next()?.parse().ok()?,
            node: parts.next()?.to_string(),
        })
    }
}

pub struct HlcClock {
    node: String,
    last: Mutex<(u64, u32)>,
}

impl HlcClock {
    pub fn new(node: String) -> Self {
        Self {
            node,
            last: Mutex::new((0, 0)),
        }
    }

    fn physical_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub fn now(&self) -> Hlc {
        self.tick(Self::physical_now())
    }

    pub(crate) fn tick(&self, physical: u64) -> Hlc {
        let mut last = self.last.lock();
        *last = if physical > last.0 {
            (physical, 0)
        } else {
            (last.0, last.1 + 1)
        };
        Hlc {
            wall_ms: last.0,
            counter: last.1,
            node: self.node.clone(),
        }
    }

    /// Advance the clock past a remote timestamp so later local ops win over it.
    pub fn observe(&self, remote: &Hlc) {
        let mut last = self.last.lock();
        if (remote.wall_ms, remote.counter) > *last {
            *last = (remote.wall_ms, remote.counter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_sorts_like_values() {
        let clock = HlcClock::new("a".into());
        let first = clock.tick(1000).encode();
        let second = clock.tick(1000).encode();
        let third = clock.tick(999).encode();
        assert!(first < second && second < third);
        assert_eq!(Hlc::decode(&third).unwrap().counter, 2);
    }

    #[test]
    fn observe_moves_clock_forward() {
        let clock = HlcClock::new("a".into());
        clock.observe(&Hlc {
            wall_ms: 5000,
            counter: 3,
            node: "b".into(),
        });
        let next = clock.tick(10);
        assert_eq!((next.wall_ms, next.counter), (5000, 4));
    }
}
