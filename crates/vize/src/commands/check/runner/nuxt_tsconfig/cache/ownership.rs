//! Fail-closed ownership checks for content-addressed cache directories.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::publish_config_atomically;

const DIGEST_LENGTH: usize = 64;
const SHARD_LENGTH: usize = 2;

enum MarkerlessDirectoryState {
    Publishable,
    Published,
    Foreign,
}

pub(in crate::commands::check::runner::nuxt_tsconfig) fn ensure_bucket(
    cache_root: &Path,
    shard: &str,
) -> Result<PathBuf, std::io::Error> {
    ensure_owned_directory(cache_root, shard, "bucket", is_shard)
}

pub(in crate::commands::check::runner::nuxt_tsconfig) fn ensure_project(
    bucket: &Path,
    digest: &str,
) -> Result<PathBuf, std::io::Error> {
    ensure_owned_directory(bucket, digest, "project", is_digest)
}

pub(in crate::commands::check::runner::nuxt_tsconfig) fn ensure_entry(
    project_cache: &Path,
    digest: &str,
) -> Result<PathBuf, std::io::Error> {
    ensure_owned_directory(project_cache, digest, "entry", is_digest)
}

pub(in crate::commands::check::runner::nuxt_tsconfig) fn validate_project(
    bucket: &Path,
    project: &Path,
) -> Result<bool, std::io::Error> {
    let Some(name) = project.file_name().and_then(|name| name.to_str()) else {
        return Ok(false);
    };
    if !is_digest(name) {
        return Ok(false);
    }
    validate_collectable_directory(bucket, project, name, "project")
}

pub(in crate::commands::check::runner::nuxt_tsconfig) fn validate_entry(
    project_cache: &Path,
    entry: &Path,
) -> Result<bool, std::io::Error> {
    let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
        return Ok(false);
    };
    if !is_digest(name) {
        return Ok(false);
    }
    validate_collectable_directory(project_cache, entry, name, "entry")
}

/// A directory whose ownership marker is missing is an interrupted creation,
/// not foreign state: collection scans skip it instead of failing the check.
fn validate_collectable_directory(
    parent: &Path,
    path: &Path,
    identity: &str,
    kind: &str,
) -> Result<bool, std::io::Error> {
    match validate_owned_directory(parent, path, identity, kind) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn ensure_owned_directory(
    parent: &Path,
    identity: &str,
    kind: &str,
    validate_identity: fn(&str) -> bool,
) -> Result<PathBuf, std::io::Error> {
    if !validate_identity(identity) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Nuxt config cache identity is not a lowercase hexadecimal identity",
        ));
    }
    let path = parent.join(identity);
    match fs::create_dir(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    validate_directory_path(parent, &path)?;
    match validate_owned_directory(parent, &path, identity, kind) {
        Ok(()) => return Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let marker_name = format!(".{kind}-owner");
    let marker = path.join(&marker_name);
    let expected = format!("vize-nuxt-{kind}:v2:{identity}\n");
    match inspect_markerless_directory(&path, &marker_name)? {
        MarkerlessDirectoryState::Publishable => {
            publish_config_atomically(&marker, expected.as_bytes())?;
        }
        MarkerlessDirectoryState::Published => {}
        MarkerlessDirectoryState::Foreign => {
            validate_or_missing_marker_error(parent, &path, identity, kind)?;
            return Ok(path);
        }
    }
    validate_owned_directory(parent, &path, identity, kind)?;
    Ok(path)
}

fn inspect_markerless_directory(
    path: &Path,
    marker_name: &str,
) -> Result<MarkerlessDirectoryState, std::io::Error> {
    let mut saw_entry = false;
    let mut has_bootstrap_lock = false;
    let mut unknown = false;
    for entry in fs::read_dir(path)?.filter_map(Result::ok) {
        saw_entry = true;
        let name = entry.file_name();
        match name.to_str() {
            Some(name) if name == marker_name => {
                return Ok(MarkerlessDirectoryState::Published);
            }
            Some(".publish.lock") => {
                has_bootstrap_lock = validate_bootstrap_lock(&entry.path())?;
                unknown |= !has_bootstrap_lock;
            }
            Some(name) if super::is_pending_name(name) => {}
            _ => unknown = true,
        }
    }
    if saw_entry && !has_bootstrap_lock {
        has_bootstrap_lock = validate_bootstrap_lock(&path.join(".publish.lock"))?;
    }
    if unknown || (saw_entry && !has_bootstrap_lock) {
        return Ok(MarkerlessDirectoryState::Foreign);
    }
    Ok(MarkerlessDirectoryState::Publishable)
}

fn validate_bootstrap_lock(path: &Path) -> Result<bool, std::io::Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Nuxt config publication lock is not a regular file",
            ))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn validate_or_missing_marker_error(
    parent: &Path,
    path: &Path,
    identity: &str,
    kind: &str,
) -> Result<(), std::io::Error> {
    match validate_owned_directory(parent, path, identity, kind) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(missing_ownership_marker_error())
        }
        Err(error) => Err(error),
    }
}

fn missing_ownership_marker_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Nuxt config cache directory has no ownership marker",
    )
}

fn validate_owned_directory(
    parent: &Path,
    path: &Path,
    identity: &str,
    kind: &str,
) -> Result<(), std::io::Error> {
    validate_directory_path(parent, path)?;
    let marker = path.join(format!(".{kind}-owner"));
    let metadata = fs::symlink_metadata(&marker)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Nuxt config cache ownership marker is not a regular file",
        ));
    }
    let expected = format!("vize-nuxt-{kind}:v2:{identity}\n");
    if fs::read(&marker)? != expected.as_bytes() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Nuxt config cache ownership marker has unexpected bytes",
        ));
    }
    Ok(())
}

fn validate_directory_path(parent: &Path, path: &Path) -> Result<(), std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Nuxt config cache path is not an owned directory",
        ));
    }
    let parent = fs::canonicalize(parent)?;
    let path = fs::canonicalize(path)?;
    if path.parent() != Some(parent.as_path()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Nuxt config cache directory escapes its owned parent",
        ));
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == DIGEST_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_shard(value: &str) -> bool {
    value.len() == SHARD_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(all(test, unix))]
#[path = "ownership_tests.rs"]
mod ownership_tests;
