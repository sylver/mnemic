use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock {
    fn unix_millis(&self) -> u64;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub unix_millis: u64,
}

impl Clock for FixedClock {
    fn unix_millis(&self) -> u64 {
        self.unix_millis
    }
}
