#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! chrono = "0.4"
//! regex = "1"
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! sha2 = "0.10"
//! urlencoding = "2"
//!
//! [package]
//! edition = "2024"
//! ```

#[path = "../../../support/common.rs"]
mod common;
#[path = "../../../support/release/preflight_matrix_evidence.rs"]
mod matrix_evidence;

use regex::Regex;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

const REQUIRED_RELEASE_WORKFLOWS: &[&str] = &[
    "Check",
    "Benchmark",
    "Fuzz",
    "Miri",
    "Real Project Matrix",
    "Docs build",
];
const PARENT_EVIDENCE_REUSABLE_WORKFLOWS: &[&str] = &[
    "Check",
    "Benchmark",
    "Fuzz",
    "Miri",
    "Real Project Matrix",
    "Docs build",
];
const RELEASE_PACKAGE_ROOTS: &[&str] = &["editors", "npm"];
const RELEASE_BLOCKING_LABELS: &[&str] = &["priority:p0", "priority:p1"];
const FAILURE_DETAIL_GITHUB_TIMEOUTS: GitHubApiTimeouts = GitHubApiTimeouts {
    connect: Duration::from_secs(5),
    total: Duration::from_secs(20),
};

#[derive(Clone, Debug)]
struct PackageManifest {
    path: String,
    content: String,
}

#[derive(Clone, Debug)]
struct GitOutput {
    status: i32,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Copy, Debug)]
struct GitHubApiTimeouts {
    connect: Duration,
    total: Duration,
}

#[derive(Clone, Debug)]
struct ReleaseTarget {
    tag: String,
    sha: String,
    version: String,
    base_sha: String,
    version_only: bool,
}

#[derive(Clone, Debug)]
struct DispatchPlan {
    workflow_name: &'static str,
    workflow_id: &'static str,
    ref_name: String,
    inputs: Value,
    expected_run_name: String,
    accepts_scheduled_evidence: bool,
}

#[derive(Clone, Debug)]
struct WorkflowEvidence {
    path: &'static str,
    events: &'static [&'static str],
    branches: &'static [(&'static str, &'static [&'static str])],
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    match parse_release_preflight_mode(&env::args().skip(1).collect::<Vec<_>>())?.as_str() {
        "target-only" => verify_release_target().map(|_| ()),
        "verify-only" => verify_release_preflight(false),
        "bootstrap" => verify_release_preflight(true),
        _ => unreachable!(),
    }
}

fn parse_release_preflight_mode(args: &[String]) -> Result<String, String> {
    match args {
        [] => Ok("bootstrap".to_string()),
        [arg] if arg == "--verify-only" => Ok("verify-only".to_string()),
        [arg] if arg == "--target-only" => Ok("target-only".to_string()),
        _ => Err(
            "Usage: rust-script tools/commands/ci/github/release-preflight.rs [--verify-only|--target-only]"
                .to_string(),
        ),
    }
}

fn verify_release_preflight(bootstrap: bool) -> Result<(), String> {
    let target = verify_release_target()?;
    let repository = env::var("GITHUB_REPOSITORY").unwrap_or_default();
    let token = env::var("GITHUB_TOKEN").unwrap_or_default();
    let api_url =
        env::var("GITHUB_API_URL").unwrap_or_else(|_| "https://api.github.com".to_string());
    if repository.is_empty() || token.is_empty() {
        return Err("GITHUB_REPOSITORY and GITHUB_TOKEN are required".to_string());
    }

    let evidence_shas = release_evidence_shas(&target);
    if target.version_only {
        println!(
            "Release {} changed version metadata only; accepting {} evidence for {}.",
            target.tag,
            target.base_sha,
            PARENT_EVIDENCE_REUSABLE_WORKFLOWS.join(", ")
        );
    }

    let dispatch_plans =
        create_release_gate_dispatch_plans(&target.tag, &target.sha, &target.base_sha)?;
    let mut runs = list_runs_for_evidence(&api_url, &repository, &token, &target, &evidence_shas)?;
    let selected = if bootstrap {
        bootstrap_required_workflow_runs(
            &api_url,
            &repository,
            &token,
            &target,
            &dispatch_plans,
            &evidence_shas,
            &mut runs,
        )?
    } else {
        select_required_workflow_runs_with_failure_details(
            &api_url,
            &repository,
            &token,
            &runs,
            &target.sha,
            &evidence_shas,
            &dispatch_plans,
        )?
    };

    let issues = github_api_pages(&api_url, &repository, &token, "issues", Some("state=open"))?;
    for (workflow_name, run) in selected
        .iter()
        .filter(|(workflow_name, _)| workflow_requires_job_evidence(workflow_name))
    {
        let run_id = run
            .get("id")
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("{workflow_name} run is missing id"))?;
        let jobs = github_api_pages(
            &api_url,
            &repository,
            &token,
            &format!("actions/runs/{run_id}/jobs"),
            None,
        )?;
        assert_required_workflow_jobs(workflow_name, &jobs)?;
    }
    if let Some(run) = selected.get(matrix_evidence::REAL_PROJECT_MATRIX_WORKFLOW_NAME) {
        let run_id = run
            .get("id")
            .and_then(Value::as_i64)
            .ok_or_else(|| "Real Project Matrix run is missing id".to_string())?;
        let artifacts = github_api_pages(
            &api_url,
            &repository,
            &token,
            &format!("actions/runs/{run_id}/artifacts"),
            None,
        )?;
        matrix_evidence::assert_real_project_matrix_release_artifacts(
            &repo_root()?,
            run,
            &artifacts,
            |artifact| matrix_evidence::download_artifact_entries(&token, artifact),
        )?;
    }
    let blockers = find_release_blockers(&issues, Some(&target.tag))?;
    if !blockers.is_empty() {
        let lines = blockers
            .iter()
            .map(|issue| {
                format!(
                    "- #{} {}",
                    issue
                        .get("number")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                    issue.get("title").and_then(Value::as_str).unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("Release-blocking issues remain open:\n{lines}"));
    }
    verify_git_release_target(&target.tag, &target.sha, &target.version)?;
    println!(
        "Release preflight passed for {} ({}) at workspace version {}.",
        target.tag, target.sha, target.version
    );
    println!(
        "Required workflows: {}",
        REQUIRED_RELEASE_WORKFLOWS.join(", ")
    );
    Ok(())
}

fn verify_release_target() -> Result<ReleaseTarget, String> {
    let tag = env::var("GITHUB_REF_NAME").unwrap_or_default();
    let sha = env::var("GITHUB_SHA").unwrap_or_default();
    if env::var("GITHUB_REF_TYPE").unwrap_or_default() != "tag" {
        return Err(format!(
            "Release preflight requires a tag event, got {}",
            env::var("GITHUB_REF_TYPE").unwrap_or_else(|_| "unknown".to_string())
        ));
    }
    let root = repo_root()?;
    let version = assert_release_metadata(
        &tag,
        &sha,
        &common::read_text(root.join("Cargo.toml"))?,
        &read_package_manifests(&root)?,
    )?;
    verify_git_release_target(&tag, &sha, &version)?;
    let base_sha = release_parent_sha(&sha)?;
    let version_only = release_changes_version_metadata_only(&base_sha, &sha);
    Ok(ReleaseTarget {
        tag,
        sha,
        version,
        base_sha,
        version_only,
    })
}

fn read_package_manifests(root: &Path) -> Result<Vec<PackageManifest>, String> {
    let files = run_git(
        &[
            "ls-files",
            "-z",
            "--",
            RELEASE_PACKAGE_ROOTS[0],
            RELEASE_PACKAGE_ROOTS[1],
        ],
        &[0],
        root,
    )?
    .stdout;
    let mut manifests = Vec::new();
    for relative in files
        .split('\0')
        .filter(|path| path.ends_with("/package.json"))
    {
        let content = common::read_text(root.join(relative))?;
        let package_json: Value = serde_json::from_str(&content).map_err(|error| {
            format!("Failed to parse tracked package manifest {relative}: {error}")
        })?;
        if package_json.get("private").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        manifests.push(PackageManifest {
            path: relative.to_string(),
            content,
        });
    }
    manifests.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(manifests)
}

fn assert_release_metadata(
    tag: &str,
    sha: &str,
    cargo_toml: &str,
    package_manifests: &[PackageManifest],
) -> Result<String, String> {
    let sha_re = Regex::new(r"^[0-9a-f]{40}$").unwrap();
    if !sha_re.is_match(sha) {
        return Err(format!("Release SHA must be a full commit SHA, got {sha}"));
    }
    parse_release_version(tag)?;
    let version = workspace_version_from_cargo_toml(cargo_toml)?;
    if tag != format!("v{version}") {
        return Err(format!(
            "Release tag {tag} does not match workspace version {version}"
        ));
    }
    let mut mismatches = Vec::new();
    for manifest in package_manifests {
        let package_json: Value = serde_json::from_str(&manifest.content).map_err(|error| {
            format!(
                "Failed to parse release package manifest {}: {error}",
                manifest.path
            )
        })?;
        if package_json.get("private").and_then(Value::as_bool) == Some(true) {
            mismatches.push(format!("{} is private", manifest.path));
            continue;
        }
        if package_json.get("version").and_then(Value::as_str) != Some(version.as_str()) {
            mismatches.push(format!(
                "{}={}",
                manifest.path,
                package_json
                    .get("version")
                    .map_or_else(|| "null".to_string(), value_to_string)
            ));
        }
    }
    if !mismatches.is_empty() {
        return Err(format!(
            "Release package versions must all equal {version}:\n{}",
            mismatches
                .iter()
                .map(|value| format!("- {value}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    Ok(version)
}

fn workspace_version_from_cargo_toml(content: &str) -> Result<String, String> {
    let mut in_workspace_package = false;
    let version_re = Regex::new(r#"^version\s*=\s*"([^"]+)"$"#).unwrap();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_workspace_package = trimmed == "[workspace.package]";
            continue;
        }
        if !in_workspace_package {
            continue;
        }
        if let Some(captures) = version_re.captures(trimmed) {
            return Ok(captures[1].to_string());
        }
    }
    Err("Cargo.toml is missing [workspace.package].version".to_string())
}

fn verify_git_release_target(tag: &str, sha: &str, version: &str) -> Result<(), String> {
    let root = repo_root()?;
    let head = run_git(&["rev-parse", "HEAD"], &[0], &root)?
        .stdout
        .trim()
        .to_string();
    if head != sha {
        return Err(format!(
            "Checked out HEAD {head} does not match release event SHA {sha}"
        ));
    }
    let main_sha = run_git(&["rev-parse", "refs/remotes/origin/main"], &[0], &root)?
        .stdout
        .trim()
        .to_string();
    let main_first_parent_history = run_git(
        &["rev-list", "--first-parent", "refs/remotes/origin/main"],
        &[0],
        &root,
    )?
    .stdout;
    assert_release_commit_is_on_main_first_parent(
        sha,
        &main_sha,
        main_first_parent_history
            .lines()
            .any(|line| line.trim() == sha),
    )?;
    let main_cargo = run_git(
        &["show", "refs/remotes/origin/main:Cargo.toml"],
        &[0],
        &root,
    )?
    .stdout;
    let main_version = workspace_version_from_cargo_toml(&main_cargo)?;
    assert_release_version_still_owns_main(tag, sha, &main_sha, version, &main_version)?;
    let remote = run_git(
        &[
            "ls-remote",
            "--exit-code",
            "--tags",
            "origin",
            &format!("refs/tags/{tag}"),
            &format!("refs/tags/{tag}^{{}}"),
        ],
        &[0],
        &root,
    )?
    .stdout;
    let target = remote_tag_commit(&remote, tag);
    if target.as_deref() != Some(sha) {
        let target_description = target.unwrap_or_else(|| "nothing".to_string());
        return Err(format!(
            "Remote tag {tag} points to {target_description}, expected {sha}"
        ));
    }
    Ok(())
}

fn release_parent_sha(sha: &str) -> Result<String, String> {
    let root = repo_root()?;
    let revision = run_git(&["rev-list", "--parents", "-n", "1", sha], &[0], &root)?.stdout;
    let parts = revision.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 2 || parts[0] != sha {
        return Err(format!(
            "Release commit {sha} must have exactly one parent for benchmark comparison"
        ));
    }
    let base_sha = parts[1].to_string();
    let ancestry = run_git(
        &["merge-base", "--is-ancestor", &base_sha, sha],
        &[0, 1],
        &root,
    )?;
    if ancestry.status != 0 {
        return Err(format!(
            "Benchmark base {base_sha} is not an ancestor of release commit {sha}"
        ));
    }
    Ok(base_sha)
}

fn release_changes_version_metadata_only(base_sha: &str, sha: &str) -> bool {
    let Ok(root) = repo_root() else {
        return false;
    };
    let Ok(diff) = run_git(
        &["diff", "--name-only", &format!("{base_sha}..{sha}")],
        &[0],
        &root,
    ) else {
        return false;
    };
    let changed = diff
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    is_version_metadata_only_release(&changed)
}

fn release_evidence_shas(target: &ReleaseTarget) -> BTreeMap<&'static str, Vec<String>> {
    if !target.version_only {
        return BTreeMap::new();
    }
    PARENT_EVIDENCE_REUSABLE_WORKFLOWS
        .iter()
        .copied()
        .map(|name| (name, vec![target.sha.clone(), target.base_sha.clone()]))
        .collect()
}

fn create_release_gate_dispatch_plans(
    ref_name: &str,
    head_sha: &str,
    base_sha: &str,
) -> Result<Vec<DispatchPlan>, String> {
    let sha_re = Regex::new(r"^[0-9a-f]{40}$").unwrap();
    for (label, value) in [("head", head_sha), ("base", base_sha)] {
        if !sha_re.is_match(value) {
            return Err(format!(
                "Release {label} SHA must be a full lowercase commit SHA, got {value}"
            ));
        }
    }
    if base_sha == head_sha {
        return Err("Release base SHA must differ from the release head SHA".to_string());
    }
    if ref_name.is_empty() {
        return Err("Release dispatch ref is required".to_string());
    }
    Ok(vec![
        DispatchPlan {
            workflow_name: "Benchmark",
            workflow_id: "benchmark.yml",
            ref_name: ref_name.to_string(),
            inputs: json!({ "base_sha": base_sha, "head_sha": head_sha }),
            expected_run_name: format!("Benchmark {base_sha}...{head_sha}"),
            accepts_scheduled_evidence: false,
        },
        DispatchPlan {
            workflow_name: "Fuzz",
            workflow_id: "fuzz.yml",
            ref_name: ref_name.to_string(),
            inputs: json!({ "mode": "replay" }),
            expected_run_name: format!("Fuzz replay @ {head_sha}"),
            accepts_scheduled_evidence: false,
        },
        DispatchPlan {
            workflow_name: "Real Project Matrix",
            workflow_id: "real-project-matrix.yml",
            ref_name: ref_name.to_string(),
            inputs: json!({
                "core_tools_mode": "record-only",
                "typecheck_dependencies_mode": "record-only",
                "lint_divergence_mode": "record-only",
                "lsp_mode": "record-only",
                "typecheck_divergence_mode": "enforce",
                "davinci_dom_corpus_mode": "record-only",
            }),
            expected_run_name: format!("Real Project Matrix @ {head_sha}"),
            accepts_scheduled_evidence: false,
        },
    ])
}

fn list_runs_for_evidence(
    api_url: &str,
    repository: &str,
    token: &str,
    target: &ReleaseTarget,
    evidence_shas: &BTreeMap<&'static str, Vec<String>>,
) -> Result<Vec<Value>, String> {
    let mut source_shas = BTreeSet::from([target.sha.clone()]);
    for shas in evidence_shas.values() {
        source_shas.extend(shas.iter().cloned());
    }
    let mut runs = Vec::new();
    for sha in source_shas {
        runs.extend(github_api_pages(
            api_url,
            repository,
            token,
            "actions/runs",
            Some(&format!("head_sha={sha}")),
        )?);
    }
    Ok(runs)
}

fn bootstrap_required_workflow_runs(
    api_url: &str,
    repository: &str,
    token: &str,
    target: &ReleaseTarget,
    dispatch_plans: &[DispatchPlan],
    evidence_shas: &BTreeMap<&'static str, Vec<String>>,
    runs: &mut Vec<Value>,
) -> Result<BTreeMap<String, Value>, String> {
    let mut dispatch_errors = Vec::new();
    for plan in dispatch_plans {
        if latest_required_workflow_run(
            runs,
            &accepted_evidence_shas(evidence_shas, plan.workflow_name, &target.sha),
            plan.workflow_name,
            Some(plan),
        )?
        .is_none()
        {
            println!(
                "Dispatching {} on {} for {}.",
                plan.workflow_name, plan.ref_name, target.sha
            );
            if let Err(error) = dispatch_workflow(api_url, repository, token, plan) {
                dispatch_errors.push(format!("{}: {error}", plan.workflow_name));
            }
        }
    }
    if !dispatch_errors.is_empty() {
        return Err(format!(
            "Failed to dispatch release gates:\n{}",
            dispatch_errors
                .iter()
                .map(|error| format!("- {error}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    let started = Instant::now();
    let timeout = Duration::from_secs(345 * 60);
    let poll_interval = Duration::from_secs(15);
    let mut previous_wait_state = String::new();
    loop {
        *runs = list_runs_for_evidence(api_url, repository, token, target, evidence_shas)?;
        let mut missing = Vec::new();
        let mut pending = Vec::new();
        let mut failed = false;
        for workflow_name in REQUIRED_RELEASE_WORKFLOWS {
            let run = latest_required_workflow_run(
                runs,
                &accepted_evidence_shas(evidence_shas, workflow_name, &target.sha),
                workflow_name,
                dispatch_plans
                    .iter()
                    .find(|plan| plan.workflow_name == *workflow_name),
            )?;
            match run {
                None => missing.push(*workflow_name),
                Some(run) if run.get("status").and_then(Value::as_str) != Some("completed") => {
                    if required_workflow_has_terminal_failed_job(
                        api_url,
                        repository,
                        token,
                        workflow_name,
                        run,
                    ) {
                        failed = true;
                    } else {
                        pending.push(*workflow_name)
                    }
                }
                Some(run) if run.get("conclusion").and_then(Value::as_str) != Some("success") => {
                    failed = true
                }
                _ => {}
            }
        }
        if failed || (missing.is_empty() && pending.is_empty()) {
            let failure_details = collect_required_workflow_failure_details(
                api_url,
                repository,
                token,
                runs,
                &target.sha,
                evidence_shas,
                dispatch_plans,
            );
            return select_required_workflow_runs(
                runs,
                &target.sha,
                evidence_shas,
                dispatch_plans,
                Some(&failure_details),
            );
        }
        if started.elapsed() >= timeout {
            let failure_details = collect_required_workflow_failure_details(
                api_url,
                repository,
                token,
                runs,
                &target.sha,
                evidence_shas,
                dispatch_plans,
            );
            return select_required_workflow_runs(
                runs,
                &target.sha,
                evidence_shas,
                dispatch_plans,
                Some(&failure_details),
            )
            .map_err(|error| {
                format!(
                    "Timed out after {}ms waiting for release gates.\n{error}",
                    timeout.as_millis()
                )
            });
        }
        let wait_state = format!("missing={missing:?};pending={pending:?}");
        if wait_state != previous_wait_state {
            previous_wait_state = wait_state;
            println!(
                "Waiting for exact-SHA release gates (missing: {}; pending: {}).",
                if missing.is_empty() {
                    "none".to_string()
                } else {
                    missing.join(", ")
                },
                if pending.is_empty() {
                    "none".to_string()
                } else {
                    pending.join(", ")
                }
            );
        }
        thread::sleep(poll_interval.min(timeout.saturating_sub(started.elapsed())));
    }
}

fn select_required_workflow_runs_with_failure_details(
    api_url: &str,
    repository: &str,
    token: &str,
    runs: &[Value],
    sha: &str,
    evidence_shas: &BTreeMap<&'static str, Vec<String>>,
    dispatch_plans: &[DispatchPlan],
) -> Result<BTreeMap<String, Value>, String> {
    let failure_details = collect_required_workflow_failure_details(
        api_url,
        repository,
        token,
        runs,
        sha,
        evidence_shas,
        dispatch_plans,
    );
    select_required_workflow_runs(
        runs,
        sha,
        evidence_shas,
        dispatch_plans,
        Some(&failure_details),
    )
}

fn dispatch_workflow(
    api_url: &str,
    repository: &str,
    token: &str,
    plan: &DispatchPlan,
) -> Result<(), String> {
    let resource = format!(
        "actions/workflows/{}/dispatches",
        urlencoding::encode(plan.workflow_id)
    );
    let body = json!({ "ref": plan.ref_name, "inputs": plan.inputs });
    github_api_request(api_url, repository, token, &resource, "POST", Some(&body)).map(|_| ())
}

fn select_required_workflow_runs(
    runs: &[Value],
    sha: &str,
    evidence_shas: &BTreeMap<&'static str, Vec<String>>,
    dispatch_plans: &[DispatchPlan],
    failure_details: Option<&BTreeMap<String, String>>,
) -> Result<BTreeMap<String, Value>, String> {
    let mut selected = BTreeMap::new();
    let mut failures = Vec::new();
    for workflow_name in REQUIRED_RELEASE_WORKFLOWS {
        let evidence = required_release_workflow_evidence(workflow_name)?;
        let accepted = accepted_evidence_shas(evidence_shas, workflow_name, sha);
        let current = latest_required_workflow_run(
            runs,
            &accepted,
            workflow_name,
            dispatch_plans
                .iter()
                .find(|plan| plan.workflow_name == *workflow_name),
        )?;
        if let Some(current) = current {
            let status = current.get("status").and_then(Value::as_str);
            let conclusion = current.get("conclusion").and_then(Value::as_str);
            if status == Some("completed") && conclusion == Some("success") {
                selected.insert((*workflow_name).to_string(), current.clone());
            } else {
                let detail = failure_details
                    .and_then(|details| details.get(*workflow_name))
                    .map(|detail| format!("; {detail}"))
                    .unwrap_or_default();
                failures.push(format!(
                    "{workflow_name}: {}/{} ({}){}",
                    status.unwrap_or("unknown"),
                    conclusion.unwrap_or("no conclusion"),
                    current
                        .get("html_url")
                        .and_then(Value::as_str)
                        .unwrap_or("no URL"),
                    detail
                ));
            }
        } else {
            failures.push(format!(
                "{workflow_name}: missing {} run from {} for {}",
                evidence.events.join("/"),
                evidence.path,
                accepted.join(" or ")
            ));
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "Required release gates are not green on {sha}:\n{}",
            failures
                .iter()
                .map(|failure| format!("- {failure}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    Ok(selected)
}

fn collect_required_workflow_failure_details(
    api_url: &str,
    repository: &str,
    token: &str,
    runs: &[Value],
    sha: &str,
    evidence_shas: &BTreeMap<&'static str, Vec<String>>,
    dispatch_plans: &[DispatchPlan],
) -> BTreeMap<String, String> {
    let mut details = BTreeMap::new();
    for workflow_name in REQUIRED_RELEASE_WORKFLOWS {
        if !workflow_requires_job_evidence(workflow_name) {
            continue;
        }
        let Ok(Some(run)) = latest_required_workflow_run(
            runs,
            &accepted_evidence_shas(evidence_shas, workflow_name, sha),
            workflow_name,
            dispatch_plans
                .iter()
                .find(|plan| plan.workflow_name == *workflow_name),
        ) else {
            continue;
        };
        if run.get("status").and_then(Value::as_str) == Some("completed")
            && run.get("conclusion").and_then(Value::as_str) == Some("success")
        {
            continue;
        }
        let Some(run_id) = run.get("id").and_then(Value::as_i64) else {
            continue;
        };
        let detail = github_api_pages_with_timeouts(
            api_url,
            repository,
            token,
            &format!("actions/runs/{run_id}/jobs"),
            None,
            FAILURE_DETAIL_GITHUB_TIMEOUTS,
        )
        .ok()
        .and_then(|jobs| summarize_required_workflow_job_failures(workflow_name, &jobs));
        if let Some(detail) = detail {
            details.insert((*workflow_name).to_string(), detail);
        }
    }
    details
}

fn required_workflow_has_terminal_failed_job(
    api_url: &str,
    repository: &str,
    token: &str,
    workflow_name: &str,
    run: &Value,
) -> bool {
    if !workflow_requires_job_evidence(workflow_name) {
        return false;
    }
    let Some(run_id) = run.get("id").and_then(Value::as_i64) else {
        return false;
    };
    github_api_pages_with_timeouts(
        api_url,
        repository,
        token,
        &format!("actions/runs/{run_id}/jobs"),
        None,
        FAILURE_DETAIL_GITHUB_TIMEOUTS,
    )
    .ok()
    .is_some_and(|jobs| required_jobs_contain_terminal_failure(workflow_name, &jobs))
}

fn latest_required_workflow_run<'a>(
    runs: &'a [Value],
    accepted_shas: &[String],
    workflow_name: &str,
    qualifier: Option<&DispatchPlan>,
) -> Result<Option<&'a Value>, String> {
    let evidence = required_release_workflow_evidence(workflow_name)?;
    let mut matches = runs
        .iter()
        .filter(|run| {
            accepted_shas
                .iter()
                .any(|sha| run.get("head_sha").and_then(Value::as_str) == Some(sha.as_str()))
                && matches_evidence(run, &evidence)
                && qualifier.map_or(true, |plan| qualifies_run(run, plan))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| compare_runs(right, left));
    Ok(matches.into_iter().next())
}

fn compare_runs(left: &Value, right: &Value) -> std::cmp::Ordering {
    run_order(left).cmp(&run_order(right))
}

fn run_order(run: &Value) -> (i64, i64, i64) {
    (
        parse_time_millis(
            run.get("run_started_at")
                .or_else(|| run.get("created_at"))
                .or_else(|| run.get("updated_at"))
                .and_then(Value::as_str)
                .unwrap_or(""),
        ),
        run.get("run_attempt").and_then(Value::as_i64).unwrap_or(0),
        run.get("id").and_then(Value::as_i64).unwrap_or(0),
    )
}

fn parse_time_millis(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp_millis())
        .unwrap_or(0)
}

fn matches_evidence(run: &Value, evidence: &WorkflowEvidence) -> bool {
    let path = run.get("path").and_then(Value::as_str);
    let event = run.get("event").and_then(Value::as_str);
    if path != Some(evidence.path) || !evidence.events.contains(&event.unwrap_or("")) {
        return false;
    }
    for (branch_event, branches) in evidence.branches {
        if Some(*branch_event) == event {
            return branches
                .iter()
                .any(|branch| run.get("head_branch").and_then(Value::as_str) == Some(*branch));
        }
    }
    true
}

fn qualifies_run(run: &Value, plan: &DispatchPlan) -> bool {
    (plan.accepts_scheduled_evidence
        && run.get("event").and_then(Value::as_str) == Some("schedule"))
        || run.get("display_title").and_then(Value::as_str) == Some(plan.expected_run_name.as_str())
}

fn accepted_evidence_shas(
    evidence_shas: &BTreeMap<&'static str, Vec<String>>,
    workflow_name: &str,
    sha: &str,
) -> Vec<String> {
    evidence_shas
        .get(workflow_name)
        .cloned()
        .unwrap_or_else(|| vec![sha.to_string()])
}

fn required_release_workflow_evidence(name: &str) -> Result<WorkflowEvidence, String> {
    match name {
        "Check" => Ok(WorkflowEvidence {
            path: ".github/workflows/check.yml",
            events: &["push"],
            branches: &[("push", &["main"])],
        }),
        "Benchmark" => Ok(WorkflowEvidence {
            path: ".github/workflows/benchmark.yml",
            events: &["workflow_dispatch"],
            branches: &[],
        }),
        "Fuzz" => Ok(WorkflowEvidence {
            path: ".github/workflows/fuzz.yml",
            events: &["schedule", "workflow_dispatch"],
            branches: &[("schedule", &["main"])],
        }),
        "Miri" => Ok(WorkflowEvidence {
            path: ".github/workflows/miri.yml",
            events: &["push"],
            branches: &[("push", &["main"])],
        }),
        "Docs build" => Ok(WorkflowEvidence {
            path: ".github/workflows/build-docs.yml",
            events: &["push", "workflow_dispatch"],
            branches: &[("push", &["main"])],
        }),
        "Real Project Matrix" => Ok(WorkflowEvidence {
            path: ".github/workflows/real-project-matrix.yml",
            events: &["schedule", "workflow_dispatch"],
            branches: &[("schedule", &["main"])],
        }),
        other => Err(format!("Release evidence is not configured for {other}")),
    }
}

fn workflow_requires_job_evidence(workflow_name: &str) -> bool {
    matches!(
        workflow_name,
        "Check" | "Benchmark" | "Fuzz" | "Real Project Matrix"
    )
}

fn required_workflow_job_names(workflow_name: &str) -> Vec<String> {
    match workflow_name {
        "Check" => vec!["test-scripts".to_string()],
        "Benchmark" => vec!["pr-benchmark-budget".to_string()],
        "Fuzz" => vec![
            "Fuzz sfc_parse".to_string(),
            "Fuzz template_lexer".to_string(),
            "Fuzz js_ts_expression".to_string(),
            "Fuzz css_parse".to_string(),
            "Fuzz template_compile".to_string(),
        ],
        "Real Project Matrix" => (0..matrix_evidence::REQUIRED_REAL_PROJECT_MATRIX_SHARD_COUNT)
            .map(|shard| {
                format!(
                    "real projects ({shard}/{})",
                    matrix_evidence::REQUIRED_REAL_PROJECT_MATRIX_SHARD_COUNT
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn assert_required_workflow_jobs(
    workflow_name: &str,
    jobs_response: &[Value],
) -> Result<(), String> {
    let job_names = required_workflow_job_names(workflow_name);
    let mut jobs = flatten_collection(jobs_response, "jobs");
    if jobs.is_empty() && jobs_response.iter().all(Value::is_object) {
        jobs.extend(jobs_response);
    }
    for name in job_names {
        let matching = jobs
            .iter()
            .filter(|job| job.get("name").and_then(Value::as_str) == Some(name.as_str()))
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(format!(
                "{workflow_name} must contain exactly one successful {name} job; found {}",
                matching.len()
            ));
        }
        let job = matching[0];
        if job.get("status").and_then(Value::as_str) != Some("completed")
            || job.get("conclusion").and_then(Value::as_str) != Some("success")
        {
            return Err(format!(
                "{workflow_name} required job {} is {}/{}",
                name,
                job.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                job.get("conclusion")
                    .and_then(Value::as_str)
                    .unwrap_or("no conclusion")
            ));
        }
    }
    Ok(())
}

fn summarize_required_workflow_job_failures(
    workflow_name: &str,
    jobs_response: &[Value],
) -> Option<String> {
    let job_names = required_workflow_job_names(workflow_name);
    if job_names.is_empty() {
        return None;
    }
    let mut jobs = flatten_collection(jobs_response, "jobs");
    if jobs.is_empty() && jobs_response.iter().all(Value::is_object) {
        jobs.extend(jobs_response);
    }
    let mut failures = Vec::new();
    for name in job_names {
        let matching = jobs
            .iter()
            .filter(|job| job.get("name").and_then(Value::as_str) == Some(name.as_str()))
            .collect::<Vec<_>>();
        match matching.as_slice() {
            [] => failures.push(format!("{name}=missing")),
            [job] => {
                let status = job
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let conclusion = job
                    .get("conclusion")
                    .and_then(Value::as_str)
                    .unwrap_or("no conclusion");
                if status != "completed" || conclusion != "success" {
                    failures.push(format!("{name}={status}/{conclusion}"));
                }
            }
            _ => failures.push(format!("{name}=duplicate({})", matching.len())),
        }
    }
    if failures.is_empty() {
        return None;
    }
    const MAX_REPORTED_JOBS: usize = 8;
    let omitted = failures.len().saturating_sub(MAX_REPORTED_JOBS);
    let mut reported = failures
        .into_iter()
        .take(MAX_REPORTED_JOBS)
        .collect::<Vec<_>>();
    if omitted > 0 {
        reported.push(format!("and {omitted} more"));
    }
    Some(format!("required jobs: {}", reported.join(", ")))
}

fn required_jobs_contain_terminal_failure(workflow_name: &str, jobs_response: &[Value]) -> bool {
    let job_names = required_workflow_job_names(workflow_name);
    if job_names.is_empty() {
        return false;
    }
    let mut jobs = flatten_collection(jobs_response, "jobs");
    if jobs.is_empty() && jobs_response.iter().all(Value::is_object) {
        jobs.extend(jobs_response);
    }
    job_names.into_iter().any(|name| {
        jobs.iter()
            .filter(|job| job.get("name").and_then(Value::as_str) == Some(name.as_str()))
            .any(|job| {
                job.get("status").and_then(Value::as_str) == Some("completed")
                    && job.get("conclusion").and_then(Value::as_str) != Some("success")
            })
    })
}

fn find_release_blockers(issues: &[Value], tag: Option<&str>) -> Result<Vec<Value>, String> {
    let readiness_labels_block = match tag {
        Some(tag) => parse_release_version(tag)?.0 >= 1,
        None => true,
    };
    Ok(issues
        .iter()
        .filter(|issue| issue.get("pull_request").is_none())
        .filter(|issue| {
            issue
                .get("title")
                .and_then(Value::as_str)
                .is_some_and(|title| title.to_lowercase().starts_with("fix(fuzz):"))
                || (readiness_labels_block
                    && issue
                        .get("labels")
                        .and_then(Value::as_array)
                        .is_some_and(|labels| {
                            labels.iter().any(|label| {
                                let name = label
                                    .as_str()
                                    .or_else(|| label.get("name").and_then(Value::as_str))
                                    .unwrap_or("")
                                    .to_lowercase();
                                RELEASE_BLOCKING_LABELS.contains(&name.as_str())
                            })
                        }))
        })
        .cloned()
        .collect())
}

fn assert_release_commit_is_on_main_first_parent(
    sha: &str,
    main_sha: &str,
    is_on_first_parent: bool,
) -> Result<(), String> {
    if is_on_first_parent {
        Ok(())
    } else {
        Err(format!(
            "Release commit {sha} is not on the first-parent history of current origin/main {main_sha}"
        ))
    }
}

fn assert_release_version_still_owns_main(
    tag: &str,
    sha: &str,
    main_sha: &str,
    release_version: &str,
    main_version: &str,
) -> Result<(), String> {
    if main_version == release_version {
        Ok(())
    } else {
        Err(format!(
            "Release {tag} ({sha}) is superseded: origin/main {main_sha} is at workspace version {main_version}, not {release_version}. Publishing it now would ship an older version after a newer one; cut the next release instead."
        ))
    }
}

fn remote_tag_commit(output: &str, tag: &str) -> Option<String> {
    let mut refs = BTreeMap::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split_whitespace();
        let Some(sha) = parts.next() else {
            continue;
        };
        let Some(reference) = parts.next() else {
            continue;
        };
        refs.insert(reference.to_string(), sha.to_string());
    }
    refs.get(&format!("refs/tags/{tag}^{{}}"))
        .or_else(|| refs.get(&format!("refs/tags/{tag}")))
        .cloned()
}

fn is_version_metadata_only_release(changed_paths: &[String]) -> bool {
    if changed_paths.is_empty() {
        return false;
    }
    let basenames = BTreeSet::from([
        "Cargo.lock",
        "Cargo.toml",
        "CHANGELOG.md",
        "extension.toml",
        "package.json",
        "pnpm-lock.yaml",
        "pnpm-workspace.yaml",
    ]);
    changed_paths.iter().all(|changed_path| {
        let basename = changed_path.rsplit('/').next().unwrap_or("");
        basenames.contains(basename)
            || (basename.starts_with("README") && basename.ends_with(".md"))
    })
}

fn parse_release_version(tag: &str) -> Result<(u64, u64, u64), String> {
    let re = Regex::new(r"^v(\d+)\.(\d+)\.(\d+)(?:[-+][0-9A-Za-z.-]+)?$").unwrap();
    let captures = re
        .captures(tag)
        .ok_or_else(|| format!("Release tag must look like vMAJOR.MINOR.PATCH, got {tag}"))?;
    Ok((
        captures[1].parse().unwrap_or(0),
        captures[2].parse().unwrap_or(0),
        captures[3].parse().unwrap_or(0),
    ))
}

fn github_api_pages(
    api_url: &str,
    repository: &str,
    token: &str,
    resource: &str,
    query: Option<&str>,
) -> Result<Vec<Value>, String> {
    github_api_pages_inner(api_url, repository, token, resource, query, None)
}

fn github_api_pages_with_timeouts(
    api_url: &str,
    repository: &str,
    token: &str,
    resource: &str,
    query: Option<&str>,
    timeouts: GitHubApiTimeouts,
) -> Result<Vec<Value>, String> {
    github_api_pages_inner(api_url, repository, token, resource, query, Some(timeouts))
}

fn github_api_pages_inner(
    api_url: &str,
    repository: &str,
    token: &str,
    resource: &str,
    query: Option<&str>,
    timeouts: Option<GitHubApiTimeouts>,
) -> Result<Vec<Value>, String> {
    let mut page = 1;
    let mut items = Vec::new();
    loop {
        let separator = if query.is_some() { "&" } else { "?" };
        let url = format!(
            "{}/repos/{}/{}{}{}per_page=100&page={}",
            api_url.trim_end_matches('/'),
            repository,
            resource,
            query.map(|query| format!("?{query}")).unwrap_or_default(),
            separator,
            page
        );
        let response = if let Some(timeouts) = timeouts {
            github_api_raw_with_timeouts(token, &url, Some(timeouts))?
        } else {
            github_api_raw(token, &url)?
        };
        let value: Value = serde_json::from_str(&response)
            .map_err(|error| format!("GitHub API returned invalid JSON for {resource}: {error}"))?;
        let page_items = flatten_collection(std::slice::from_ref(&value), collection_for(resource));
        if page_items.is_empty() {
            if page == 1 {
                items.extend(value.as_array().cloned().unwrap_or_default());
            }
            break;
        }
        let count = page_items.len();
        items.extend(page_items.into_iter().cloned());
        if count < 100 {
            break;
        }
        page += 1;
    }
    Ok(items)
}

fn github_api_request(
    api_url: &str,
    repository: &str,
    token: &str,
    resource: &str,
    method: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let url = format!(
        "{}/repos/{}/{}",
        api_url.trim_end_matches('/'),
        repository,
        resource
    );
    let mut command = Command::new("curl");
    command
        .args([
            "--fail-with-body",
            "--silent",
            "--show-error",
            "--request",
            method,
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--header",
            &format!("Authorization: Bearer {token}"),
        ])
        .stdin(Stdio::null());
    let body_text;
    if let Some(body) = body {
        body_text = serde_json::to_string(body).map_err(|error| error.to_string())?;
        command.args([
            "--header",
            "Content-Type: application/json",
            "--data",
            &body_text,
        ]);
    }
    command.arg(&url);
    let output = command
        .output()
        .map_err(|error| format!("failed to run curl: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_str(&stdout)
            .map_err(|error| format!("GitHub API returned invalid JSON for {resource}: {error}"))
    }
}

fn github_api_raw(token: &str, url: &str) -> Result<String, String> {
    github_api_raw_with_timeouts(token, url, None)
}

fn github_api_raw_with_timeouts(
    token: &str,
    url: &str,
    timeouts: Option<GitHubApiTimeouts>,
) -> Result<String, String> {
    let mut command = Command::new("curl");
    command
        .args([
            "--fail-with-body",
            "--silent",
            "--show-error",
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--header",
            &format!("Authorization: Bearer {token}"),
        ])
        .stdin(Stdio::null());
    if let Some(timeouts) = timeouts {
        command
            .arg("--connect-timeout")
            .arg(curl_timeout_arg(timeouts.connect))
            .arg("--max-time")
            .arg(curl_timeout_arg(timeouts.total));
    }
    let output = command
        .arg(url)
        .output()
        .map_err(|error| format!("failed to run curl: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn curl_timeout_arg(duration: Duration) -> String {
    duration.as_secs().max(1).to_string()
}

fn collection_for(resource: &str) -> &'static str {
    if resource == "actions/runs" {
        "workflow_runs"
    } else if resource.ends_with("/jobs") {
        "jobs"
    } else if resource.ends_with("/artifacts") {
        "artifacts"
    } else {
        ""
    }
}

fn flatten_collection<'a>(values: &'a [Value], collection: &str) -> Vec<&'a Value> {
    let mut items = Vec::new();
    for value in values {
        if collection.is_empty() {
            if let Some(array) = value.as_array() {
                items.extend(array);
            }
        } else if let Some(array) = value.get(collection).and_then(Value::as_array) {
            items.extend(array);
        } else if let Some(array) = value.as_array() {
            items.extend(array);
        }
    }
    items
}

fn run_git(args: &[&str], accepted_exit_codes: &[i32], cwd: &Path) -> Result<GitOutput, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("git {} failed to run: {error}", args.join(" ")))?;
    let status = output.status.code().unwrap_or(1);
    let result = GitOutput {
        status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    if !accepted_exit_codes.contains(&status) {
        let detail = format!("{}{}", result.stdout, result.stderr);
        return Err(format!(
            "git {} failed with exit {}{}",
            args.join(" "),
            status,
            if detail.trim().is_empty() {
                String::new()
            } else {
                format!("\n{}", detail.trim())
            }
        )
        .trim()
        .to_string());
    }
    Ok(result)
}

fn repo_root() -> Result<PathBuf, String> {
    common::repo_root().or_else(|_| {
        Path::new(file!())
            .ancestors()
            .find(|candidate| {
                candidate.join("Cargo.toml").is_file()
                    && candidate.join("pnpm-workspace.yaml").is_file()
            })
            .map(Path::to_path_buf)
            .ok_or_else(|| "cannot resolve Vize repository root from script path".to_string())
    })
}

fn value_to_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}
