//! Shared by `benches/wide_int.rs` (the benchmark) and `tests/wide_int.rs` (the checks): the
//! candidate wide types and the hot loop written over them. Lives under `benches/` only; the
//! `palhunt` library and the `search` command do not use it.
//!
//! Each of the two binaries uses a different part of this module, so unused-item warnings are
//! silenced here rather than splitting the module in two.
#![allow(dead_code)]

pub mod engine;
pub mod reference;
pub mod u256;
