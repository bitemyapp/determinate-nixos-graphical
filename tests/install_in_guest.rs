#!/usr/bin/env -S rust-script --force
//! install-in-guest: executable Rust entry point; shared implementation in ../rust.
//!
//! ```cargo
//! [dependencies]
//! anyhow = "=1.0.100"
//! respin-tools = { path = "../rust", features = ["calamares"] }
//! ```
fn main() -> anyhow::Result<()> {
    respin_tools::dispatch("install-in-guest", std::env::args().skip(1).collect())
}
