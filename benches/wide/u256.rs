//! The hand-rolled u256 chosen in P1-2 now lives in `src/u256.rs` and is the production type
//! for 127 to 255 bits. Re-exported here so the benchmark and tests keep their import paths.

pub use palhunt::u256::U256;
