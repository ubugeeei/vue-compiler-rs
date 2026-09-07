#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! edition = "2024"
//!
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! ```

use std::{
    env, fs,
    process::{Command, ExitCode},
};

#[path = "../../support/common.rs"]
mod common;

const USAGE: &str =
    "usage: rust-script tools/commands/davinci/consumer-migration-surfaces.rs --write | --check";
const LEGACY_GENERATOR: &str = "legacy-tools/davinci/consumer-migration-surfaces.mjs";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<u8, String> {
    let mode = env::args().nth(1).ok_or_else(|| USAGE.to_string())?;
    if mode != "--write" && mode != "--check" {
        return Err(USAGE.to_string());
    }

    let root = common::repo_root()?;
    let generator = root.join(LEGACY_GENERATOR);
    if !generator.is_file() {
        return Err(format!(
            "cannot find consumer migration surface generator: {}",
            generator.display()
        ));
    }
    if mode == "--write" {
        fs::create_dir_all(root.join("davinci-road/plan"))
            .map_err(|error| format!("cannot create davinci-road/plan: {error}"))?;
    }

    let status = Command::new("node")
        .arg(&generator)
        .arg(&mode)
        .current_dir(&root)
        .status()
        .map_err(|error| format!("failed to run node {} {mode}: {error}", generator.display()))?;

    Ok(status.code().unwrap_or(1).try_into().unwrap_or(1))
}
