use core::time::Duration;

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Timespec {
    pub sec: i64,  // Seconds - >= 0
    pub nsec: i32, // Nanoseconds - [0, 999999999]
}

impl Timespec {
    pub fn unix_timestamp(&self) -> u32 {
        self.sec as u32
    }

    pub fn is_zero(&self) -> bool {
        self.sec == 0 && self.nsec == 0
    }

    pub fn to_duration(&self) -> Duration {
        Duration::new(self.sec as u64, self.nsec as u32)
    }
}

impl From<u32> for Timespec {
    fn from(unix_timestamp: u32) -> Self {
        Self {
            sec: unix_timestamp as i64,
            nsec: 0,
        }
    }
}
