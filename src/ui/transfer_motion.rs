use std::time::{Duration, Instant};

const TRANSITION: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, PartialEq)]
pub struct Reading {
    pub bytes: u64,
    pub speed: f64,
}

pub struct Motion {
    from: Reading,
    target: Reading,
    started: Instant,
}

impl Motion {
    pub fn new(reading: Reading, now: Instant) -> Self {
        Self {
            from: reading,
            target: reading,
            started: now,
        }
    }

    pub fn update(&mut self, target: Reading, now: Instant) -> (Reading, bool) {
        let current = self.read(now);
        if self.target != target {
            self.from = current;
            self.target = target;
            self.started = now;
        }
        (current, current != self.target)
    }

    fn read(&self, now: Instant) -> Reading {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed >= TRANSITION {
            return self.target;
        }
        let delta = self.from.bytes.abs_diff(self.target.bytes);
        let moved = (u128::from(delta) * elapsed.as_nanos() / TRANSITION.as_nanos()) as u64;
        let bytes = if self.target.bytes >= self.from.bytes {
            self.from.bytes + moved
        } else {
            self.from.bytes - moved
        };
        let fraction = elapsed.as_secs_f64() / TRANSITION.as_secs_f64();
        Reading {
            bytes,
            speed: self.from.speed + (self.target.speed - self.from.speed) * fraction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_interpolates_between_reports_and_stops_at_confirmed_bytes() {
        let start = Instant::now();
        let zero = Reading {
            bytes: 0,
            speed: 0.,
        };
        let target = Reading {
            bytes: 1000,
            speed: 100.,
        };
        let mut motion = Motion::new(zero, start);
        assert!(!motion.update(zero, start).1);
        assert!(motion.update(target, start).1);
        let middle = start + TRANSITION / 2;
        let (visible, animating) = motion.update(target, middle);
        assert_eq!(visible.bytes, 500);
        assert_eq!(visible.speed, 50.);
        assert!(animating);

        let next = Reading {
            bytes: 2000,
            speed: 200.,
        };
        assert_eq!(motion.update(next, middle).0.bytes, 500);
        assert_eq!(motion.update(next, middle + TRANSITION / 2).0.bytes, 1250);
        let (visible, animating) = motion.update(next, middle + TRANSITION * 2);
        assert_eq!(visible.bytes, 2000);
        assert!(!animating);
    }
}
