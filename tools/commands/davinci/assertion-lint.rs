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
    collections::{BTreeMap, BTreeSet, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[path = "../../support/common.rs"]
mod common;

#[derive(Clone, Debug)]
struct Finding {
    line: usize,
    category: String,
    text: String,
    offset: usize,
}

#[derive(Debug)]
struct Target {
    file: PathBuf,
    rel: String,
    whole_file: bool,
}

#[derive(Debug)]
struct Args {
    list: bool,
    root: Option<PathBuf>,
    allowlist: Option<PathBuf>,
    help: bool,
}

#[derive(Debug)]
struct AllowEntry {
    expires: String,
    used: bool,
}

#[derive(Debug)]
enum TomlValue {
    String(String),
    Array(Vec<TomlValue>),
}

const USAGE: &str = "Usage: rust-script tools/commands/davinci/assertion-lint.rs [--list] [--root <dir>] [--allowlist <file>]\n\nScans Rust test code for banned weak-assertion patterns (Davinci assurance\ndoctrine). Without flags: scans crates/**, applies the committed allowlist,\nexits 1 on unlisted findings. --list ignores the allowlist and exits 0.\n--root scans an alternate directory tree (self-test hook) with no default\nallowlist.";

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err((code, message)) => {
            eprintln!("{message}");
            if code == 2 {
                eprintln!("{USAGE}");
            }
            ExitCode::from(code)
        }
    }
}

fn run() -> Result<u8, (u8, String)> {
    let repo_root = common::repo_root().map_err(|error| (2, error))?;
    let args = parse_args(env::args().skip(1).collect())?;
    if args.help {
        println!("{USAGE}");
        return Ok(0);
    }

    let default_tree = args.root.is_none();
    let root = args.root.unwrap_or_else(|| repo_root.clone());
    if !root.exists() {
        return Err((2, format!("scan root does not exist: {}", root.display())));
    }

    let mut findings = Vec::new();
    for target in collect_targets(&root, default_tree).map_err(|error| (2, error))? {
        let source = fs::read_to_string(&target.file)
            .map_err(|error| (2, format!("cannot read {}: {error}", target.file.display())))?;
        for finding in scan_file(&source, target.whole_file) {
            findings.push((target.rel.clone(), finding));
        }
    }

    if args.list {
        for (path, finding) in &findings {
            println!(
                "{}:{} [{}] {}",
                path, finding.line, finding.category, finding.text
            );
        }
        let files = findings
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<BTreeSet<_>>();
        println!(
            "assertion-lint: {} findings in {} files (allowlist ignored)",
            findings.len(),
            files.len()
        );
        return Ok(0);
    }

    let default_allowlist = repo_root.join("davinci-road/plan/assertion-allowlist.toml");
    let allowlist_path = args
        .allowlist
        .or_else(|| (default_tree && default_allowlist.exists()).then_some(default_allowlist));
    let mut allowlist = if let Some(path) = allowlist_path {
        load_allowlist(&path).map_err(|error| (2, error))?
    } else {
        BTreeMap::new()
    };

    let today = current_yyyy_mm_dd();
    let mut unlisted = Vec::new();
    let mut expired = BTreeSet::new();
    let mut suppressed = 0usize;
    let mut suppressed_files = BTreeSet::new();
    for (path, finding) in findings {
        if let Some(entry) = allowlist.get_mut(&path) {
            if entry.expires >= today {
                entry.used = true;
                suppressed += 1;
                suppressed_files.insert(path);
                continue;
            }
            expired.insert(path.clone());
        }
        unlisted.push((path, finding));
    }

    for (path, finding) in &unlisted {
        let note = if expired.contains(path) {
            " (allowlist entry expired)"
        } else {
            ""
        };
        println!(
            "{}:{} [{}] {}{}",
            path, finding.line, finding.category, finding.text, note
        );
    }
    for (path, entry) in &allowlist {
        if !entry.used && !expired.contains(path) {
            eprintln!(
                "warning: allowlist entry {path} matched no finding — remove it (the list only shrinks)"
            );
        }
    }

    if !unlisted.is_empty() {
        println!(
            "assertion-lint: {} unlisted findings — fix the assertion (exact oracles only) or triage via davinci-road/plan/assertion-allowlist.toml",
            unlisted.len()
        );
        return Ok(1);
    }
    println!(
        "assertion-lint: OK ({} findings in {} files suppressed by allowlist)",
        suppressed,
        suppressed_files.len()
    );
    Ok(0)
}

fn parse_args(argv: Vec<String>) -> Result<Args, (u8, String)> {
    let mut args = Args {
        list: false,
        root: None,
        allowlist: None,
        help: false,
    };
    let mut index = 0;
    while index < argv.len() {
        match argv[index].as_str() {
            "--list" => args.list = true,
            "--root" => {
                index += 1;
                let value = argv
                    .get(index)
                    .ok_or_else(|| (2, "--root requires a directory argument".to_string()))?;
                args.root = Some(PathBuf::from(value));
            }
            "--allowlist" => {
                index += 1;
                let value = argv
                    .get(index)
                    .ok_or_else(|| (2, "--allowlist requires a file argument".to_string()))?;
                args.allowlist = Some(PathBuf::from(value));
            }
            "--help" | "-h" => args.help = true,
            other => return Err((2, format!("unknown argument {other}"))),
        }
        index += 1;
    }
    Ok(args)
}

fn current_yyyy_mm_dd() -> String {
    common::run_capture("date", &["+%Y-%m-%d"])
        .map(|output| output.stdout.trim().to_string())
        .unwrap_or_else(|_| "1970-01-01".to_string())
}

fn load_allowlist(path: &Path) -> Result<BTreeMap<String, AllowEntry>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read allowlist {}: {error}", path.display()))?;
    let mut groups = Vec::<BTreeMap<String, TomlValue>>::new();
    let mut current: Option<BTreeMap<String, TomlValue>> = None;
    let lines: Vec<&str> = text.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let line_no = index + 1;
        let line = strip_comment(lines[index]).trim().to_string();
        if line.is_empty() {
            index += 1;
            continue;
        }
        if line == "[[allow]]" {
            if let Some(group) = current.take() {
                groups.push(group);
            }
            current = Some(BTreeMap::new());
            index += 1;
            continue;
        }
        let Some(group) = current.as_mut() else {
            return Err(format!(
                "malformed allowlist {}: line {line_no}: assignment outside [[allow]]",
                path.display()
            ));
        };
        let (key, value) = line.split_once('=').ok_or_else(|| {
            format!(
                "malformed allowlist {}: line {line_no}: expected key = value",
                path.display()
            )
        })?;
        let key = key.trim().to_string();
        if group.contains_key(&key) {
            return Err(format!(
                "malformed allowlist {}: line {line_no}: duplicate key {key}",
                path.display()
            ));
        }
        let mut value_text = value.trim().to_string();
        if value_text.starts_with('[') && !array_closed(&value_text) {
            while index + 1 < lines.len() {
                index += 1;
                value_text.push('\n');
                value_text.push_str(lines[index]);
                if array_closed(&value_text) {
                    break;
                }
            }
        }
        group.insert(
            key,
            parse_toml_value(&value_text).map_err(|error| {
                format!(
                    "malformed allowlist {}: line {line_no}: {error}",
                    path.display()
                )
            })?,
        );
        index += 1;
    }
    if let Some(group) = current.take() {
        groups.push(group);
    }

    let mut by_path = BTreeMap::new();
    for (group_index, group) in groups.iter().enumerate() {
        let where_ = format!("{} group {}", path.display(), group_index + 1);
        match group.get("justification") {
            Some(TomlValue::String(value)) if !value.trim().is_empty() => {}
            _ => return Err(format!("{where_}: justification is required")),
        }
        let expires = match group.get("expires") {
            Some(TomlValue::String(value)) if is_date(value) => value.clone(),
            _ => {
                return Err(format!(
                    "{where_}: expires must be a quoted YYYY-MM-DD date"
                ));
            }
        };
        let paths = match group.get("paths") {
            Some(TomlValue::Array(values)) if !values.is_empty() => values,
            _ => return Err(format!("{where_}: paths must be a non-empty array")),
        };
        for value in paths {
            let TomlValue::String(entry_path) = value else {
                return Err(format!(
                    "{where_}: paths must be repo-root-relative forward-slash strings"
                ));
            };
            if !is_allowlist_path(entry_path) {
                return Err(format!(
                    "{where_}: paths must be normalized repo-root-relative forward-slash strings"
                ));
            }
            if by_path
                .insert(
                    entry_path.clone(),
                    AllowEntry {
                        expires: expires.clone(),
                        used: false,
                    },
                )
                .is_some()
            {
                return Err(format!("{where_}: duplicate path {entry_path}"));
            }
        }
    }
    Ok(by_path)
}

fn is_allowlist_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn parse_toml_value(value: &str) -> Result<TomlValue, String> {
    let value = strip_comment(value).trim().to_string();
    if value.starts_with('"') {
        let (string, rest) = read_basic_string(&value)?;
        if !strip_comment(rest).trim().is_empty() {
            return Err("unexpected trailing content".to_string());
        }
        return Ok(TomlValue::String(string));
    }
    if value.starts_with('[') {
        let mut items = Vec::new();
        let mut rest = value[1..].to_string();
        loop {
            rest = strip_comment(&rest).trim_start().to_string();
            if rest.starts_with(']') {
                if !strip_comment(&rest[1..]).trim().is_empty() {
                    return Err("unexpected trailing content".to_string());
                }
                return Ok(TomlValue::Array(items));
            }
            if rest.is_empty() {
                return Err("unterminated array".to_string());
            }
            if !rest.starts_with('"') {
                return Err("paths array entries must be strings".to_string());
            }
            let (string, after) = read_basic_string(&rest)?;
            items.push(TomlValue::String(string));
            rest = strip_comment(after).trim_start().to_string();
            if rest.starts_with(',') {
                rest = rest[1..].to_string();
            } else if !rest.starts_with(']') {
                return Err("expected `,` or `]` in array".to_string());
            }
        }
    }
    Err("unsupported value".to_string())
}

fn array_closed(value: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    for byte in value.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b']' {
            return true;
        }
    }
    false
}

fn strip_comment(value: &str) -> String {
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in value.bytes().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else if byte == b'"' {
            in_string = true;
        } else if byte == b'#' {
            return value[..index].to_string();
        }
    }
    value.to_string()
}

fn read_basic_string(value: &str) -> Result<(String, &str), String> {
    let bytes = value.as_bytes();
    if bytes.first() != Some(&b'"') {
        return Err("expected string".to_string());
    }
    let mut result = String::new();
    let mut index = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'"' {
            return Ok((result, &value[index + 1..]));
        }
        if byte == b'\\' {
            index += 1;
            let Some(&escape) = bytes.get(index) else {
                return Err("unterminated escape".to_string());
            };
            match escape {
                b'"' => result.push('"'),
                b'\\' => result.push('\\'),
                b'n' => result.push('\n'),
                b'r' => result.push('\r'),
                b't' => result.push('\t'),
                _ => return Err(format!("unsupported escape \\{}", escape as char)),
            }
        } else {
            result.push(byte as char);
        }
        index += 1;
    }
    Err("unterminated basic string".to_string())
}

fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

fn collect_targets(root: &Path, default_tree: bool) -> Result<Vec<Target>, String> {
    let mut files = Vec::new();
    if default_tree {
        walk_rust_files(&root.join("crates"), &mut files)?;
    } else {
        walk_rust_files(root, &mut files)?;
    }
    let mut targets = Vec::new();
    for file in files {
        let rel = common::relative_path(root, &file);
        if default_tree {
            if is_crate_integration_test(&rel) {
                targets.push(Target {
                    file,
                    rel,
                    whole_file: true,
                });
            } else if is_crate_source(&rel) {
                targets.push(Target {
                    file,
                    rel,
                    whole_file: false,
                });
            }
        } else {
            let whole_file = rel.split('/').any(|part| part == "tests");
            targets.push(Target {
                file,
                rel,
                whole_file,
            });
        }
    }
    targets.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(targets)
}

fn is_crate_integration_test(rel: &str) -> bool {
    let parts = rel.split('/').collect::<Vec<_>>();
    parts.len() >= 4 && parts[0] == "crates" && parts[2] == "tests" && rel.ends_with(".rs")
}

fn is_crate_source(rel: &str) -> bool {
    let parts = rel.split('/').collect::<Vec<_>>();
    parts.len() >= 4 && parts[0] == "crates" && parts[2] == "src" && rel.ends_with(".rs")
}

fn walk_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(dir)
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules" | "dist") {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot stat {}: {error}", path.display()))?;
        if file_type.is_dir() {
            walk_rust_files(&path, files)?;
        } else if file_type.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("rs")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn scan_file(source: &str, whole_file: bool) -> Vec<Finding> {
    let masked = mask_rust_source(source);
    let regions = if whole_file {
        vec![(0, masked.len())]
    } else {
        cfg_test_mod_regions(&masked)
    };
    if regions.is_empty() {
        return Vec::new();
    }
    let spans = assert_spans(&masked, &regions);
    if spans.is_empty() {
        return Vec::new();
    }
    let starts = line_starts(source);
    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    for (from, to) in spans {
        let partial_json = contains_json_macro(&masked[from..to]);
        for category in ["contains", "starts-with", "ends-with", "regex"] {
            for hit in find_category_hits(&masked, from, to, category) {
                let line = line_number_at(&starts, hit);
                let reported = if category == "contains" && partial_json {
                    "partial-json"
                } else {
                    category
                };
                let key = format!("{line}:{reported}");
                if !seen.insert(key) {
                    continue;
                }
                let line_end = starts
                    .get(line)
                    .copied()
                    .map(|offset| offset.saturating_sub(1))
                    .unwrap_or(source.len());
                findings.push(Finding {
                    line,
                    category: reported.to_string(),
                    text: source[starts[line - 1]..line_end].trim().to_string(),
                    offset: hit,
                });
            }
        }
    }
    findings.sort_by_key(|finding| finding.offset);
    findings
}

fn mask_rust_source(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        if byte == b'/' && next == Some(b'/') {
            let mut end = index;
            while end < bytes.len() && bytes[end] != b'\n' {
                end += 1;
            }
            blank(&mut out, index, end);
            index = end;
            continue;
        }
        if byte == b'/' && next == Some(b'*') {
            let mut depth = 1usize;
            let mut end = index + 2;
            while end < bytes.len() && depth > 0 {
                if bytes[end] == b'/' && bytes.get(end + 1) == Some(&b'*') {
                    depth += 1;
                    end += 2;
                } else if bytes[end] == b'*' && bytes.get(end + 1) == Some(&b'/') {
                    depth -= 1;
                    end += 2;
                } else {
                    end += 1;
                }
            }
            blank(&mut out, index, end);
            index = end;
            continue;
        }
        if matches!(byte, b'r' | b'b' | b'c')
            && !is_ident_byte(bytes.get(index.wrapping_sub(1)).copied())
        {
            if let Some((body_start, body_end, next_index)) = raw_string_bounds(bytes, index) {
                blank(&mut out, body_start, body_end);
                index = next_index;
                continue;
            }
        }
        if byte == b'"' {
            let mut end = index + 1;
            while end < bytes.len() {
                if bytes[end] == b'\\' {
                    end += 2;
                } else if bytes[end] == b'"' {
                    end += 1;
                    break;
                } else {
                    end += 1;
                }
            }
            blank(&mut out, index + 1, end.saturating_sub(1));
            index = end;
            continue;
        }
        if byte == b'\'' {
            if let Some(end) = char_literal_end(bytes, index) {
                blank(&mut out, index + 1, end.saturating_sub(1));
                index = end;
                continue;
            }
        }
        index += 1;
    }
    String::from_utf8(out).expect("masked source remains utf8")
}

fn blank(out: &mut [u8], start: usize, end: usize) {
    let out_len = out.len();
    for byte in out.iter_mut().take(end.min(out_len)).skip(start) {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn raw_string_bounds(bytes: &[u8], start: usize) -> Option<(usize, usize, usize)> {
    let mut index = start;
    if bytes.get(index) == Some(&b'b') || bytes.get(index) == Some(&b'c') {
        index += 1;
    }
    if bytes.get(index) != Some(&b'r') {
        return None;
    }
    index += 1;
    let hash_start = index;
    while bytes.get(index) == Some(&b'#') {
        index += 1;
    }
    if bytes.get(index) != Some(&b'"') {
        return None;
    }
    let hashes = index - hash_start;
    let body_start = index + 1;
    let mut scan = body_start;
    while scan < bytes.len() {
        if bytes[scan] == b'"'
            && scan + hashes < bytes.len()
            && bytes[scan + 1..scan + 1 + hashes]
                .iter()
                .all(|byte| *byte == b'#')
        {
            return Some((body_start, scan, scan + 1 + hashes));
        }
        scan += 1;
    }
    Some((body_start, bytes.len(), bytes.len()))
}

fn char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    if start + 2 >= bytes.len() {
        return None;
    }
    if bytes[start + 1] == b'\\' {
        let mut index = start + 2;
        if bytes.get(index) == Some(&b'u') && bytes.get(index + 1) == Some(&b'{') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'}' {
                index += 1;
            }
            index += 1;
        } else {
            index += 1;
        }
        return (bytes.get(index) == Some(&b'\'')).then_some(index + 1);
    }
    (bytes[start + 1] != b'\''
        && bytes[start + 1] != b'\\'
        && bytes[start + 1] != b'\n'
        && bytes[start + 2] == b'\'')
        .then_some(start + 3)
}

fn cfg_test_mod_regions(masked: &str) -> Vec<(usize, usize)> {
    let bytes = masked.as_bytes();
    let mut regions = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let Some(attr_end) = match_cfg_test_attr(bytes, index) else {
            index += 1;
            continue;
        };
        let mut item = attr_end;
        loop {
            item = skip_ws(bytes, item);
            if bytes.get(item) == Some(&b'#') {
                if let Some(end) = match_outer_attr(bytes, item) {
                    item = end;
                    continue;
                }
            }
            break;
        }
        if let Some(open) = match_mod_head(bytes, item) {
            if let Some(end) = match_delimiter(bytes, open, b'{', b'}') {
                regions.push((open + 1, end - 1));
                index = end;
                continue;
            }
        }
        index = attr_end;
    }
    regions
}

fn match_cfg_test_attr(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'#') {
        return None;
    }
    let mut index = start + 1;
    index = skip_ws(bytes, index);
    if bytes.get(index) != Some(&b'[') {
        return None;
    }
    let end = match_delimiter(bytes, index, b'[', b']')?;
    let inner = String::from_utf8_lossy(&bytes[index + 1..end - 1])
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    (inner == "cfg(test)").then_some(end)
}

fn match_outer_attr(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    index = skip_ws(bytes, index);
    (bytes.get(index) == Some(&b'[')).then(|| match_delimiter(bytes, index, b'[', b']'))?
}

fn match_mod_head(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    if match_word(bytes, index, b"pub") {
        index += 3;
        index = skip_ws(bytes, index);
        if bytes.get(index) == Some(&b'(') {
            index = match_delimiter(bytes, index, b'(', b')')?;
            index = skip_ws(bytes, index);
        }
    }
    if !match_word(bytes, index, b"mod") {
        return None;
    }
    index += 3;
    if !bytes
        .get(index)
        .copied()
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        return None;
    }
    index = skip_ws(bytes, index);
    if !bytes.get(index).copied().is_some_and(is_ident_start) {
        return None;
    }
    index += 1;
    while bytes
        .get(index)
        .copied()
        .is_some_and(|byte| is_ident_byte(Some(byte)))
    {
        index += 1;
    }
    index = skip_ws(bytes, index);
    (bytes.get(index) == Some(&b'{')).then_some(index)
}

fn assert_spans(masked: &str, regions: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let bytes = masked.as_bytes();
    let macros = [
        b"debug_assert_eq!".as_slice(),
        b"debug_assert_ne!".as_slice(),
        b"debug_assert!".as_slice(),
        b"assert_eq!".as_slice(),
        b"assert_ne!".as_slice(),
        b"assert!".as_slice(),
    ];
    let mut spans = Vec::new();
    for &(start, end) in regions {
        let mut index = start;
        while index < end {
            let mut matched = None;
            for name in macros {
                if index + name.len() <= bytes.len()
                    && &bytes[index..index + name.len()] == name
                    && !is_ident_byte(bytes.get(index.wrapping_sub(1)).copied())
                {
                    matched = Some(name.len());
                    break;
                }
            }
            let Some(len) = matched else {
                index += 1;
                continue;
            };
            let open = skip_ws(bytes, index + len);
            if bytes.get(open) != Some(&b'(') {
                index += len;
                continue;
            }
            let Some(close) = match_delimiter(bytes, open, b'(', b')') else {
                index += len;
                continue;
            };
            if close <= end {
                spans.push((open + 1, close - 1));
                index = close;
            } else {
                index += len;
            }
        }
    }
    spans
}

fn find_category_hits(masked: &str, from: usize, to: usize, category: &str) -> Vec<usize> {
    let bytes = masked.as_bytes();
    let mut hits = Vec::new();
    let mut index = from;
    while index < to {
        let matched = match category {
            "contains" => match_dot_call(bytes, index, b"contains"),
            "starts-with" => match_dot_call(bytes, index, b"starts_with"),
            "ends-with" => match_dot_call(bytes, index, b"ends_with"),
            "regex" => match_regex_new(bytes, index),
            _ => None,
        };
        if let Some(end) = matched {
            hits.push(index);
            index = end.max(index + 1);
        } else {
            index += 1;
        }
    }
    hits
}

fn match_dot_call(bytes: &[u8], start: usize, name: &[u8]) -> Option<usize> {
    if bytes.get(start) != Some(&b'.') {
        return None;
    }
    let mut index = skip_ws(bytes, start + 1);
    if index + name.len() > bytes.len() || &bytes[index..index + name.len()] != name {
        return None;
    }
    index += name.len();
    index = skip_ws(bytes, index);
    (bytes.get(index) == Some(&b'(')).then_some(index + 1)
}

fn match_regex_new(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    if !match_word(bytes, index, b"Regex") {
        return None;
    }
    index += 5;
    index = skip_ws(bytes, index);
    if bytes.get(index) != Some(&b':') || bytes.get(index + 1) != Some(&b':') {
        return None;
    }
    index = skip_ws(bytes, index + 2);
    if !match_word(bytes, index, b"new") {
        return None;
    }
    Some(index + 3)
}

fn contains_json_macro(span: &str) -> bool {
    let bytes = span.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if match_word(bytes, index, b"json") {
            let mut cursor = skip_ws(bytes, index + 4);
            if bytes.get(cursor) == Some(&b'!') {
                cursor = skip_ws(bytes, cursor + 1);
                if bytes
                    .get(cursor)
                    .is_some_and(|byte| matches!(byte, b'(' | b'[' | b'{'))
                {
                    return true;
                }
            }
        }
        index += 1;
    }
    false
}

fn match_delimiter(bytes: &[u8], open_index: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0isize;
    for (index, byte) in bytes.iter().enumerate().skip(open_index) {
        if *byte == open {
            depth += 1;
        } else if *byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(index + 1);
            }
        }
    }
    None
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn line_number_at(starts: &[usize], offset: usize) -> usize {
    match starts.binary_search(&offset) {
        Ok(index) => index + 1,
        Err(index) => index,
    }
}

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while bytes
        .get(index)
        .copied()
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    index
}

fn match_word(bytes: &[u8], start: usize, word: &[u8]) -> bool {
    start + word.len() <= bytes.len()
        && &bytes[start..start + word.len()] == word
        && !is_ident_byte(bytes.get(start.wrapping_sub(1)).copied())
        && !is_ident_byte(bytes.get(start + word.len()).copied())
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_byte(byte: Option<u8>) -> bool {
    byte.is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
