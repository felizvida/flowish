use crate::hash::{StableHasher, stable_hash_bytes};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitMask {
    len: usize,
    words: Vec<u64>,
}

impl BitMask {
    pub fn zeros(len: usize) -> Self {
        let word_count = len.div_ceil(64);
        Self {
            len,
            words: vec![0; word_count],
        }
    }

    pub fn from_predicate(len: usize, mut predicate: impl FnMut(usize) -> bool) -> Self {
        let mut mask = Self::zeros(len);
        for index in 0..len {
            if predicate(index) {
                mask.set(index, true);
            }
        }
        mask
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn set(&mut self, index: usize, value: bool) {
        assert!(index < self.len, "bit index out of bounds");
        let word_index = index / 64;
        let bit_offset = index % 64;
        let bit = 1u64 << bit_offset;
        if value {
            self.words[word_index] |= bit;
        } else {
            self.words[word_index] &= !bit;
        }
    }

    pub fn contains(&self, index: usize) -> bool {
        assert!(index < self.len, "bit index out of bounds");
        let word_index = index / 64;
        let bit_offset = index % 64;
        (self.words[word_index] >> bit_offset) & 1 == 1
    }

    pub fn count_ones(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    pub fn and(&self, other: &Self) -> Self {
        assert_eq!(self.len, other.len, "bitmask lengths must match");
        let words = self
            .words
            .iter()
            .zip(other.words.iter())
            .map(|(left, right)| left & right)
            .collect();
        Self {
            len: self.len,
            words,
        }
    }

    pub fn iter_ones(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.len).filter(|index| self.contains(*index))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.words.len() * 8);
        bytes.extend_from_slice(&(self.len as u64).to_le_bytes());
        for word in &self.words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    pub fn stable_hash(&self) -> u64 {
        stable_hash_bytes(&self.to_bytes())
    }

    pub fn stable_hash_hex(&self) -> String {
        let mut hasher = StableHasher::new();
        hasher.update(&self.to_bytes());
        hasher.finish_hex()
    }
}

#[cfg(test)]
mod tests {
    use super::BitMask;

    #[test]
    fn supports_large_masks() {
        let mask = BitMask::from_predicate(130, |index| index % 3 == 0);
        assert_eq!(mask.count_ones(), 44);
        assert!(mask.contains(129));
        assert!(!mask.contains(128));
    }

    #[test]
    fn and_combines_masks_deterministically() {
        let left = BitMask::from_predicate(8, |index| index % 2 == 0);
        let right = BitMask::from_predicate(8, |index| index >= 4);
        let combined = left.and(&right);
        let indices = combined.iter_ones().collect::<Vec<_>>();
        assert_eq!(indices, vec![4, 6]);
    }
}
