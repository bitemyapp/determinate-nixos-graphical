#!/usr/bin/env -S rust-script --force
//! test-handoff: executable Rust entry point; shared implementation in ../rust.
//!
//! ```cargo
//! [dependencies]
//! anyhow = "=1.0.100"
//! respin-tools = { path = "../rust", features = ["calamares"] }
//! ```
fn main() -> anyhow::Result<()> {
    respin_tools::dispatch("test-handoff", std::env::args().skip(1).collect())
}
