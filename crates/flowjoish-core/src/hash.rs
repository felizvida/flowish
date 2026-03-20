use std::fmt::Write;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Debug)]
pub struct StableHasher {
    state: u64,
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StableHasher {
    pub fn new() -> Self {
        Self {
            state: FNV_OFFSET_BASIS,
        }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.update_chunk(&(bytes.len() as u64).to_le_bytes());
        self.update_chunk(bytes);
    }

    pub fn update_u64(&mut self, value: u64) {
        self.update(&value.to_le_bytes());
    }

    pub fn update_bool(&mut self, value: bool) {
        self.update(&[u8::from(value)]);
    }

    pub fn update_str(&mut self, value: &str) {
        self.update(value.as_bytes());
    }

    pub fn finish_u64(&self) -> u64 {
        self.state
    }

    pub fn finish_hex(&self) -> String {
        let mut output = String::with_capacity(16);
        let _ = write!(&mut output, "{:016x}", self.state);
        output
    }

    fn update_chunk(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }
}

pub fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.update(bytes);
    hasher.finish_u64()
}

pub fn stable_hash_str(value: &str) -> u64 {
    stable_hash_bytes(value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{StableHasher, stable_hash_str};

    #[test]
    fn hashes_are_stable() {
        let first = stable_hash_str("flowjoish");
        let second = stable_hash_str("flowjoish");
        assert_eq!(first, second);
    }

    #[test]
    fn chunk_boundaries_change_hashes() {
        let mut split = StableHasher::new();
        split.update(b"ab");
        split.update(b"c");

        let mut combined = StableHasher::new();
        combined.update(b"abc");

        assert_ne!(split.finish_u64(), combined.finish_u64());
    }
}
