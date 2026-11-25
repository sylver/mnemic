use crate::clock::Clock;

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub fn is_ulid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 26 && bytes.iter().all(|byte| CROCKFORD.contains(byte))
}

#[derive(Debug, Clone)]
pub struct UlidGenerator {
    state: u64,
}

impl UlidGenerator {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    pub fn next(&mut self, clock: &impl Clock) -> String {
        let timestamp = clock.unix_millis() & ((1 << 48) - 1);
        let random = (u128::from(self.next_u64()) << 16) | u128::from(self.next_u64() >> 48);
        encode(((timestamp as u128) << 80) | (random & ((1u128 << 80) - 1)))
    }

    fn next_u64(&mut self) -> u64 {
        let mut state = self.state;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.state = state;
        state
    }
}

fn encode(mut value: u128) -> String {
    let mut chars = [b'0'; 26];
    for slot in chars.iter_mut().rev() {
        *slot = CROCKFORD[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8_lossy(&chars).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;

    #[test]
    fn same_seed_and_clock_repeat() {
        let clock = FixedClock {
            unix_millis: 1_763_865_600_000,
        };
        let mut first = UlidGenerator::from_seed(7);
        let mut second = UlidGenerator::from_seed(7);
        let a = first.next(&clock);
        let b = second.next(&clock);
        assert_eq!(a, b);
        assert!(is_ulid(&a));
        assert_ne!(first.next(&clock), a);
    }

    #[test]
    fn rejects_lowercase_and_short_ids() {
        assert!(!is_ulid("01arz3ndektsv4rrffq69g5fav"));
        assert!(!is_ulid("01ARZ3NDEKTSV4RRFFQ69G5FA"));
        assert!(is_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
    }
}
