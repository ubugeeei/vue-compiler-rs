#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! edition = "2024"
//!
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! ```

use std::{env, process::ExitCode};

#[path = "../../support/artifacts.rs"]
mod artifact_command;

fn main() -> ExitCode {
    artifact_command::run_node_generator(
        env::args().nth(1).as_deref(),
        "legacy-tools/davinci/sourcelocation-inventory.mjs",
        "usage: rust-script tools/commands/davinci/sourcelocation-inventory.rs --write | --check",
    )
}
