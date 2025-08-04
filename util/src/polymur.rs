use std::{hash::{BuildHasher, Hasher}, sync::LazyLock};

use polymur_hash::PolymurHash;
use rand::random;

static HASH: LazyLock<PolymurHash> = LazyLock::new(|| PolymurHash::new(random()));

#[derive(Debug, Clone)]
pub struct RandomState {
    tweak: u64
}

#[derive(Debug, Clone)]
pub struct PolymurHasher {
    tweak: u64,
    hash: Option<u64>
}

impl RandomState {
    pub fn new() -> Self {
        Self { tweak: random() }
    }
}

impl Default for RandomState {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildHasher for RandomState {
    type Hasher = PolymurHasher;

    fn build_hasher(&self) -> Self::Hasher {
        PolymurHasher { tweak: self.tweak, hash: None }
    }
}

impl PolymurHasher {
    pub const fn new() -> Self {
        Self { tweak: 0, hash: None }
    }
}

impl Default for PolymurHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for PolymurHasher {
    fn finish(&self) -> u64 {
        self.hash.expect("invariant violated")
    }

    fn write(&mut self, i: &[u8]) {
        assert!(self.hash.is_none(), "invariant violated");
        self.hash = Some(HASH.hash_with_tweak(i, self.tweak))
    }
}