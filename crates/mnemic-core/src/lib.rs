use std::time::SystemTime;

use thiserror::Error;

pub mod write;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0} is empty")]
    Empty(&'static str),
    /// Another process holds the vault write lock after the wait elapsed.
    #[error("lock held by pid {pid} since {}", since_secs(.since))]
    LockHeld { pid: u32, since: SystemTime },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn since_secs(since: &SystemTime) -> u64 {
    since
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty(left), Self::Empty(right)) => left == right,
            (
                Self::LockHeld {
                    pid: left_pid,
                    since: left_since,
                },
                Self::LockHeld {
                    pid: right_pid,
                    since: right_since,
                },
            ) => left_pid == right_pid && left_since == right_since,
            (Self::Io(left), Self::Io(right)) => {
                left.kind() == right.kind() && left.to_string() == right.to_string()
            }
            _ => false,
        }
    }
}

impl Eq for Error {}

pub fn require_non_empty<'a>(label: &'static str, value: &'a str) -> Result<&'a str, Error> {
    if value.is_empty() {
        Err(Error::Empty(label))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_value() {
        assert_eq!(require_non_empty("name", ""), Err(Error::Empty("name")));
    }

    #[test]
    fn keeps_non_empty_value() {
        assert_eq!(require_non_empty("name", "mnemic"), Ok("mnemic"));
    }
}
