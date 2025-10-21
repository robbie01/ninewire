#![forbid(unsafe_code)]

use std::{hash::{BuildHasher as _, Hasher as _, RandomState}, hint::black_box, time::Instant};

use rand::random;
use util::polymur;

const ITERATIONS: u32 = 100000000;

fn main() {
    let h = RandomState::new();
    let t1 = Instant::now();
    for _ in 0..ITERATIONS {
        let n = random::<u64>();
        let mut h = h.build_hasher();
        h.write_u64(n);
        black_box(h.finish());
    }
    let t2 = Instant::now();
    println!("default: {} ns/iter", ((t2 - t1) / ITERATIONS).as_nanos());


    let h = polymur::RandomState::new();
    let t1 = Instant::now();
    for _ in 0..ITERATIONS {
        let n = random::<u64>();
        let mut h = h.build_hasher();
        h.write_u64(n);
        black_box(h.finish());
    }
    let t2 = Instant::now();
    println!("polymur: {} ns/iter", ((t2 - t1) / ITERATIONS).as_nanos());
}
