#![allow(dead_code)]

use std::{
    fs,
    path::Path,
    process::{Command, ExitCode},
};

#[path = "./common.rs"]
mod common;

pub fn run_single(
    mode: Option<&str>,
    rel_path: &str,
    usage: &str,
    regen_command: &str,
) -> Result<(), String> {
    let root = common::repo_root()?;
    let path = root.join(rel_path);
    match mode {
        Some("--write") => {
            let text = common::read_text(&path)?;
            common::write_text(&path, &text)?;
            println!("wrote {rel_path}");
            Ok(())
        }
        Some("--check") => {
            if !path.exists() {
                return Err(format!(
                    "stale: {rel_path} does not exist. Regenerate with: {regen_command}"
                ));
            }
            println!("{rel_path} is up to date");
            Ok(())
        }
        _ => Err(usage.to_string()),
    }
}

pub fn run_many(
    mode: Option<&str>,
    rel_paths: &[&str],
    usage: &str,
    ok_message: &str,
) -> Result<(), String> {
    let root = common::repo_root()?;
    match mode {
        Some("--write") => {
            for rel_path in rel_paths {
                let path = root.join(rel_path);
                let text = common::read_text(&path)?;
                common::write_text(&path, &text)?;
                println!("wrote {rel_path}");
            }
            Ok(())
        }
        Some("--check") => {
            for rel_path in rel_paths {
                if !root.join(rel_path).exists() {
                    return Err(format!("stale: {rel_path} does not exist"));
                }
            }
            println!("{ok_message}");
            Ok(())
        }
        _ => Err(usage.to_string()),
    }
}

pub fn run_node_generator(mode: Option<&str>, generator_rel_path: &str, usage: &str) -> ExitCode {
    match run_node_generator_result(mode, generator_rel_path, usage) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_node_generator_result(
    mode: Option<&str>,
    generator_rel_path: &str,
    usage: &str,
) -> Result<u8, String> {
    let mode = mode.ok_or_else(|| usage.to_string())?;
    if mode != "--write" && mode != "--check" {
        return Err(usage.to_string());
    }

    let root = common::repo_root()?;
    let generator = root.join(generator_rel_path);
    if !generator.is_file() {
        return Err(format!(
            "cannot find Davinci artifact generator: {}",
            generator.display()
        ));
    }
    if mode == "--write" {
        fs::create_dir_all(root.join("davinci-road/plan"))
            .map_err(|error| format!("cannot create davinci-road/plan: {error}"))?;
    }

    let status = Command::new("node")
        .arg(&generator)
        .arg(mode)
        .current_dir(&root)
        .status()
        .map_err(|error| format!("failed to run node {} {mode}: {error}", generator.display()))?;

    Ok(status.code().unwrap_or(1).try_into().unwrap_or(1))
}

pub fn copy_artifact(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::copy(from, to).map(|_| ()).map_err(|error| {
        format!(
            "cannot copy {} to {}: {error}",
            from.display(),
            to.display()
        )
    })
}
