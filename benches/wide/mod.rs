//! Shared by `benches/wide_int.rs` (the benchmark) and `tests/wide_int.rs` (the checks). Since
//! P1-3 (#26) the hand-rolled u256 and the hot loop are production code in `src/` (re-exported
//! by `engine` and `u256`); what stays here is the `bnum` comparison type and the slow,
//! independent reference.
//!
//! Each of the two binaries uses a different part of this module, so unused-item warnings are
//! silenced here rather than splitting the module in two.
#![allow(dead_code, unused_imports)]

pub mod engine;
pub mod reference;
pub mod u256;
