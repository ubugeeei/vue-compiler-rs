#!/usr/bin/env rust-script
//! ```cargo
//! [package]
//! edition = "2024"
//!
//! [dependencies]
//! pathdiff = "0.2"
//! regex = "1"
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! sha2 = "0.10"
//! ```

use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[path = "../../support/common.rs"]
mod common;

#[derive(Debug)]
struct Args {
    budget_mode: String,
    documented_differences: PathBuf,
    registry: PathBuf,
    report_dir: PathBuf,
    shard_count: usize,
    shard_index: usize,
    vize_bin: PathBuf,
    vue_tsc_bin: PathBuf,
}

fn main() -> ExitCode {
    common::main_result(run())
}

fn run() -> Result<(), String> {
    let root = common::repo_root()?;
    let args = parse_args(env::args().skip(1).collect(), &root)?;
    let registry = common::read_json(&args.registry)?;
    let selected = select_typecheck_projects(&registry, &args)?;
    if selected.is_empty() {
        println!(
            "No typecheck performance projects selected for shard {}/{}",
            args.shard_index, args.shard_count
        );
        return Ok(());
    }
    let mut artifacts = Vec::new();
    let mut failures = Vec::new();
    for project in selected {
        match write_project_artifact(&root, &args, project) {
            Ok(artifact) => {
                if let Some(detail) = budget_failure_detail(&artifact) {
                    failures.push(detail);
                }
                artifacts.push(artifact);
            }
            Err(error) => return Err(error),
        }
    }
    if !failures.is_empty() && args.budget_mode == "enforce" {
        return Err(failures.join("\n"));
    }
    if !failures.is_empty() {
        for failure in failures {
            println!("::warning title=Typecheck divergence budget not enforced::{failure}");
        }
    }
    Ok(())
}

fn write_project_artifact(root: &Path, args: &Args, project: &Value) -> Result<Value, String> {
    validate_performance(project)?;
    let project_id = project_string(project, "id")?;
    let fixture_root = root.join(project_string(project, "fixturePath")?);
    let summary = read_and_validate_summary(root, &args.report_dir, project)?;
    let vize_run = read_and_validate_vize_run(root, &args.report_dir, project, &summary)?;
    let preparation =
        read_and_validate_dependency_preparation(root, &args.report_dir, project, &summary)?;
    let source_project = project
        .pointer("/typecheckPerformance/baseline/tsconfig")
        .and_then(Value::as_str)
        .or_else(|| project.get("tsconfig").and_then(Value::as_str))
        .ok_or_else(|| {
            format!(
                "{} is missing tsconfig",
                project_string(project, "id").unwrap_or_else(|_| "project".to_string())
            )
        })?
        .to_string();
    let source_config = common::read_text(fixture_root.join(&source_project))?;
    let typecheck_source_roots = source_roots(&fixture_root, project, &vize_run.payload["parsed"])?;
    let baseline_config = materialize_baseline_project(
        root,
        &fixture_root,
        &args.report_dir,
        project,
        &source_project,
        &vize_run.payload["parsed"],
        &typecheck_source_roots,
    )?;
    let vue_tsc_version = run_capture_limited(
        &args.vue_tsc_bin,
        &["--version".to_string()],
        root,
        10_000,
        "vue-tsc",
        &[0],
    )?
    .stdout
    .trim()
    .to_string();
    if vue_tsc_version.is_empty() {
        return Err(format!(
            "vue-tsc is not runnable: {}",
            args.vue_tsc_bin.display()
        ));
    }
    let performance = project.get("typecheckPerformance").ok_or_else(|| {
        format!(
            "{} has no typecheckPerformance",
            project_string(project, "id").unwrap_or_else(|_| "project".to_string())
        )
    })?;
    let timeout_ms = performance
        .get("hangTimeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(5_000);
    let progress_enabled = env::var_os("VIZE_TYPECHECK_DIVERGENCE_PROGRESS").is_some();
    if progress_enabled {
        eprintln!("[typecheck-divergence] start projectId={project_id} timeoutMs={timeout_ms}");
    }
    let baseline_args = vec![
        "--noEmit".to_string(),
        "--pretty".to_string(),
        "false".to_string(),
        "-p".to_string(),
        baseline_config.path.display().to_string(),
    ];
    let coverage_args = vec![
        "--noEmit".to_string(),
        "--pretty".to_string(),
        "false".to_string(),
        "--listFilesOnly".to_string(),
        "-p".to_string(),
        baseline_config.path.display().to_string(),
    ];
    if progress_enabled {
        eprintln!("[typecheck-divergence] run projectId={project_id} command=baseline");
    }
    let baseline = run_baseline_command(
        &args.vue_tsc_bin,
        &baseline_args,
        &fixture_root,
        timeout_ms,
        "vue-tsc baseline",
        &[0, 1, 2],
    )?;
    if progress_enabled {
        eprintln!(
            "[typecheck-divergence] finish projectId={project_id} command=baseline durationMs={} status={}",
            baseline.run.duration_ms, baseline.run.status
        );
        eprintln!("[typecheck-divergence] run projectId={project_id} command=coverage");
    }
    let coverage_baseline = if let Some(reason) = baseline.run_error.as_deref() {
        skipped_baseline_command(
            &args.vue_tsc_bin,
            &coverage_args,
            "vue-tsc coverage baseline",
            &format!("vue-tsc coverage baseline skipped because {reason}"),
        )
    } else {
        run_baseline_command(
            &args.vue_tsc_bin,
            &coverage_args,
            &fixture_root,
            timeout_ms,
            "vue-tsc coverage baseline",
            &[0, 1, 2],
        )?
    };
    if progress_enabled {
        eprintln!(
            "[typecheck-divergence] finish projectId={project_id} command=coverage durationMs={} status={}",
            coverage_baseline.run.duration_ms, coverage_baseline.run.status
        );
    }
    let documented_differences = read_documented_differences(&args.documented_differences)?;
    let expected_documented_differences =
        select_documented_differences(&documented_differences, &project_id, &fixture_root)?;
    let divergence = compare_typecheck_diagnostics(
        project_id.clone(),
        &fixture_root,
        &vize_run.payload["parsed"],
        &baseline.run.output,
        &typecheck_source_roots,
        &documented_differences,
    )?;
    let coverage = if let Some(reason) = baseline
        .run_error
        .as_deref()
        .or(coverage_baseline.run_error.as_deref())
    {
        unavailable_vue_program_coverage(&vize_run.payload["parsed"], &fixture_root, reason)?
    } else {
        evaluate_vue_program_coverage(
            &vize_run.payload["parsed"],
            &coverage_baseline.run.output,
            &fixture_root,
            &typecheck_source_roots,
        )?
    };
    let configuration = if let Some(reason) = baseline.run_error.as_deref() {
        unavailable_baseline_configuration(reason)
    } else {
        evaluate_baseline_configuration(&baseline.run.output)?
    };
    let ambient = if let Some(reason) = coverage_baseline.run_error.as_deref() {
        unavailable_baseline_ambient_environment(reason)
    } else {
        evaluate_baseline_ambient_environment(&coverage_baseline.run.output, &fixture_root)?
    };
    let mutation_oracle = create_seeded_mutation_oracle(MutationContext {
        project,
        fixture_root: &fixture_root,
        vize_report: &vize_run.payload["parsed"],
        coverage: &coverage,
        configuration: &configuration,
        baseline_config: &baseline_config,
        vize_bin: &args.vize_bin,
        vue_tsc_bin: &args.vue_tsc_bin,
        documented_differences: &documented_differences,
    })?;
    reject_stale_documented_differences(
        &project_id,
        &expected_documented_differences,
        &divergence,
        &mutation_oracle,
    )?;
    let budget = evaluate_budget(
        performance,
        &divergence,
        &coverage,
        &configuration,
        &mutation_oracle,
        &ambient,
    )?;
    let artifact = json!({
        "schema": "vize.fixtureTypecheckDivergenceRun",
        "version": 7,
        "project": project_id.clone(),
        "revision": project.get("revision").cloned().unwrap_or(Value::Null),
        "tsconfig": source_project,
        "evidence": summary["evidence"],
        "enforcement": { "budgetMode": args.budget_mode },
        "preparation": preparation,
        "source": vize_run.source,
        "baseline": {
            "command": display_command(&args.vue_tsc_bin, &baseline_args),
            "coverageCommand": display_command(&args.vue_tsc_bin, &coverage_args),
            "configSha256": sha256(&baseline_config.source),
            "sourceConfigSha256": sha256(&source_config),
            "version": vue_tsc_version,
            "durationMs": baseline.run.duration_ms,
            "coverageDurationMs": coverage_baseline.run.duration_ms,
            "exitCode": captured_exit_code(&baseline),
            "coverageExitCode": captured_exit_code(&coverage_baseline),
            "runError": baseline.run_error,
            "coverageRunError": coverage_baseline.run_error,
            "ambient": ambient,
            "configuration": configuration,
            "coverage": coverage,
            "stdoutSha256": sha256(&baseline.run.stdout),
            "stderrSha256": sha256(&baseline.run.stderr),
            "coverageStdoutSha256": sha256(&coverage_baseline.run.stdout),
            "coverageStderrSha256": sha256(&coverage_baseline.run.stderr),
        },
        "mutationOracle": mutation_oracle,
        "budget": budget,
        "divergence": divergence,
    });
    let json_path = args
        .report_dir
        .join(format!("{project_id}-typecheck-divergence.json"));
    let markdown_path = args
        .report_dir
        .join(format!("{project_id}-typecheck-divergence.md"));
    common::write_json_pretty(&json_path, &artifact)?;
    common::write_text(&markdown_path, &render_markdown(&artifact))?;
    println!("Wrote {}", display_relative(root, &json_path));
    println!("Wrote {}", display_relative(root, &markdown_path));
    Ok(artifact)
}

struct BaselineConfig {
    path: PathBuf,
    source: String,
}

struct VizeRun {
    payload: Value,
    source: Value,
}

fn read_and_validate_summary(
    root: &Path,
    report_dir: &Path,
    project: &Value,
) -> Result<Value, String> {
    let summary = common::read_json(report_dir.join("summary.json"))?;
    if summary.get("schema").and_then(Value::as_str) != Some("vize.fixtureToolMatrixReport")
        || summary.get("version").and_then(Value::as_u64) != Some(3)
    {
        return Err("Fixture matrix summary schema is unsupported".to_string());
    }
    let commit_sha = summary
        .pointer("/evidence/commitSha")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !is_full_sha(commit_sha) {
        return Err("Fixture matrix summary is missing exact commit evidence".to_string());
    }
    if let Ok(expected) = env::var("GITHUB_SHA") {
        if expected != commit_sha {
            return Err("Fixture matrix summary commit does not match GITHUB_SHA".to_string());
        }
    }
    let project_id = project_string(project, "id")?;
    let empty_projects = Vec::new();
    let matches = summary
        .get("projects")
        .and_then(Value::as_array)
        .unwrap_or(&empty_projects)
        .iter()
        .filter(|entry| entry.get("id").and_then(Value::as_str) == Some(project_id.as_str()))
        .collect::<Vec<_>>();
    if matches.len() != 1 || matches[0].get("revision") != project.get("revision") {
        return Err(format!(
            "Fixture matrix summary does not contain pinned project {project_id}"
        ));
    }
    let _ = root;
    Ok(summary)
}

fn read_and_validate_vize_run(
    root: &Path,
    report_dir: &Path,
    project: &Value,
    summary: &Value,
) -> Result<VizeRun, String> {
    let project_id = project_string(project, "id")?;
    let project_summary = summary
        .get("projects")
        .and_then(Value::as_array)
        .and_then(|projects| {
            projects
                .iter()
                .find(|entry| entry.get("id").and_then(Value::as_str) == Some(project_id.as_str()))
        })
        .ok_or_else(|| {
            format!("Fixture matrix summary does not contain pinned project {project_id}")
        })?;
    let empty_runs = Vec::new();
    let runs = project_summary
        .get("runs")
        .and_then(Value::as_array)
        .unwrap_or(&empty_runs)
        .iter()
        .filter(|run| run.get("tool").and_then(Value::as_str) == Some("typechecker"))
        .collect::<Vec<_>>();
    if runs.len() != 1
        || !matches!(
            runs[0].get("status").and_then(Value::as_str),
            Some("ok" | "findings")
        )
    {
        return Err(format!(
            "Fixture matrix summary has no successful typechecker run for {project_id}"
        ));
    }
    let expected_name = format!("{project_id}-typechecker.json");
    let reported_path = runs[0]
        .get("outputPath")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!("Fixture matrix typechecker output path is invalid for {project_id}")
        })?;
    let artifact_path = fs::canonicalize(root.join(reported_path)).map_err(|_| {
        format!("Fixture matrix typechecker output path is invalid for {project_id}")
    })?;
    let expected_path = fs::canonicalize(report_dir.join(&expected_name)).map_err(|_| {
        format!("Fixture matrix typechecker output path is invalid for {project_id}")
    })?;
    if artifact_path != expected_path
        || Path::new(reported_path)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(expected_name.as_str())
    {
        return Err(format!(
            "Fixture matrix typechecker output path is invalid for {project_id}"
        ));
    }
    let raw_payload = common::read_text(&artifact_path)?;
    let payload = serde_json::from_str::<Value>(&raw_payload).map_err(|error| {
        format!("Fixture matrix typechecker JSON is invalid for {project_id}: {error}")
    })?;
    require_exact_keys(
        &payload,
        &[
            "exitCode",
            "parsed",
            "project",
            "schema",
            "stderr",
            "stdout",
            "tool",
            "typecheckerCoverage",
            "version",
        ],
        &format!("Fixture matrix typechecker artifact keys are invalid for {project_id}"),
    )?;
    if payload.get("schema").and_then(Value::as_str) != Some("vize.fixtureToolRun")
        || payload.get("version").and_then(Value::as_u64) != Some(1)
        || payload.get("project").and_then(Value::as_str) != Some(project_id.as_str())
        || payload.get("tool").and_then(Value::as_str) != Some("typechecker")
        || payload.get("exitCode") != runs[0].get("exitCode")
    {
        return Err(format!(
            "Fixture matrix typechecker artifact identity is invalid for {project_id}"
        ));
    }
    let stdout =
        serde_json::from_str::<Value>(payload.get("stdout").and_then(Value::as_str).unwrap_or(""))
            .map_err(|_| {
                format!("Fixture matrix typechecker stdout is not JSON for {project_id}")
            })?;
    if canonical_json(&stdout) != canonical_json(&payload["parsed"]) {
        return Err(format!(
            "Fixture matrix typechecker stdout does not match parsed output for {project_id}"
        ));
    }
    let expected_status = if payload.get("exitCode").and_then(Value::as_i64) == Some(0) {
        "ok"
    } else {
        "findings"
    };
    if runs[0].get("status").and_then(Value::as_str) != Some(expected_status) {
        return Err(format!(
            "Fixture matrix typechecker status is inconsistent for {project_id}"
        ));
    }
    if runs[0].get("fileCount").and_then(Value::as_u64)
        != payload.pointer("/parsed/fileCount").and_then(Value::as_u64)
    {
        return Err(format!(
            "Fixture matrix typechecker file count is inconsistent for {project_id}"
        ));
    }
    let fixture_root = root.join(project_string(project, "fixturePath")?);
    let expected_files = expected_typecheck_vue_files(&fixture_root, project, &payload["parsed"])?;
    let authored_files = collect_typechecker_authored_paths(&fixture_root)?;
    let expected_coverage = validate_typechecker_output(
        project,
        &payload["parsed"],
        payload.get("exitCode").and_then(Value::as_i64).unwrap_or(1) as i32,
        Some(&expected_files),
        Some(&authored_files),
    )?;
    if canonical_json(&expected_coverage) != canonical_json(&payload["typecheckerCoverage"]) {
        return Err(format!(
            "Fixture matrix typechecker coverage is inconsistent for {project_id}"
        ));
    }
    let expected_summary = summarize_typechecker_coverage(&expected_coverage)?;
    if canonical_json(&expected_summary)
        != canonical_json(runs[0].get("coverage").unwrap_or(&Value::Null))
    {
        return Err(format!(
            "Fixture matrix typechecker summary coverage is inconsistent for {project_id}"
        ));
    }
    let source = json!({
        "payloadSha256": sha256(&raw_payload),
        "fileCount": payload.pointer("/parsed/fileCount").and_then(Value::as_u64).unwrap_or(0),
    });
    Ok(VizeRun { payload, source })
}

fn read_and_validate_dependency_preparation(
    root: &Path,
    report_dir: &Path,
    project: &Value,
    summary: &Value,
) -> Result<Value, String> {
    let project_id = project_string(project, "id")?;
    let path = report_dir.join(format!("{project_id}-typecheck-dependencies.json"));
    let raw = common::read_text(&path).map_err(|_| {
        format!("Missing typecheck dependency preparation evidence for {project_id}")
    })?;
    let artifact = serde_json::from_str::<Value>(&raw).map_err(|error| {
        format!("Invalid typecheck dependency preparation JSON for {project_id}: {error}")
    })?;
    if artifact.get("schema").and_then(Value::as_str)
        != Some("vize.fixtureTypecheckDependencyInstall")
        || artifact.get("version").and_then(Value::as_u64) != Some(2)
        || artifact.get("project").and_then(Value::as_str) != Some(project_id.as_str())
        || artifact.get("revision") != project.get("revision")
        || artifact.pointer("/evidence/commitSha") != summary.pointer("/evidence/commitSha")
    {
        return Err(format!(
            "Typecheck dependency preparation identity is invalid for {project_id}"
        ));
    }
    let fixture_root = root.join(project_string(project, "fixturePath")?);
    let lockfile_path = artifact
        .pointer("/lockfile/path")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!("Typecheck dependency lockfile evidence is invalid for {project_id}")
        })?;
    let lockfile = fs::read(fixture_root.join(lockfile_path)).map_err(|error| {
        format!("Typecheck dependency lockfile evidence is invalid for {project_id}: {error}")
    })?;
    if artifact
        .pointer("/lockfile/sizeBytes")
        .and_then(Value::as_u64)
        != Some(lockfile.len() as u64)
        || artifact.pointer("/lockfile/sha256").and_then(Value::as_str)
            != Some(sha256_bytes(&lockfile).as_str())
    {
        return Err(format!(
            "Typecheck dependency lockfile evidence is invalid for {project_id}"
        ));
    }
    Ok(json!({
        "schema": "vize.fixtureTypecheckPreparationEvidence",
        "version": 1,
        "payloadSha256": sha256(&raw),
        "packageManager": artifact["packageManager"],
        "lockfile": artifact["lockfile"],
        "install": {
            "command": artifact.pointer("/install/command").cloned().unwrap_or(Value::Null),
            "exitCode": artifact.pointer("/install/exitCode").cloned().unwrap_or(Value::Null),
            "stdoutSha256": artifact.pointer("/install/stdoutSha256").cloned().unwrap_or(Value::Null),
            "stderrSha256": artifact.pointer("/install/stderrSha256").cloned().unwrap_or(Value::Null),
        },
        "baselinePrepare": artifact.get("baselinePrepare").and_then(|value| {
            if value.is_null() {
                Some(Value::Null)
            } else {
                Some(json!({
                    "command": value.get("command").cloned().unwrap_or(Value::Null),
                    "exitCode": value.get("exitCode").cloned().unwrap_or(Value::Null),
                    "stdoutSha256": value.get("stdoutSha256").cloned().unwrap_or(Value::Null),
                    "stderrSha256": value.get("stderrSha256").cloned().unwrap_or(Value::Null),
                }))
            }
        }).unwrap_or(Value::Null),
    }))
}

fn materialize_baseline_project(
    root: &Path,
    fixture_root: &Path,
    report_dir: &Path,
    project: &Value,
    source_project: &str,
    vize_report: &Value,
    source_roots: &[PathBuf],
) -> Result<BaselineConfig, String> {
    let source_path = fixture_root.join(source_project);
    let source_dir = source_path
        .parent()
        .ok_or_else(|| "source tsconfig has no parent".to_string())?;
    let config_dir = source_dir.join(".vize-baseline");
    let project_id = project_string(project, "id")?;
    let output_path = config_dir.join(format!("{project_id}-vue-tsc.tsconfig.json"));
    let artifact_path = report_dir.join(format!("{project_id}-vue-tsc.tsconfig.json"));
    fs::create_dir_all(&config_dir)
        .map_err(|error| format!("cannot create {}: {error}", config_dir.display()))?;
    let source_document = read_tsconfig_jsonc(&source_path)?;
    let compiler_paths = winning_compiler_paths(fixture_root, &source_path)?;
    let mut dot_roots = dot_directory_include_roots(fixture_root, vize_report);
    let fixture_roots = [fixture_root.to_path_buf()];
    dot_roots.extend(discover_dot_directory_include_roots(&fixture_roots)?);
    dot_roots.extend(tsconfig_include_dot_roots(
        fixture_root,
        source_dir,
        &source_document,
    )?);
    dedup_paths(&mut dot_roots);
    let mut ambient_roots = vec![source_dir.to_path_buf()];
    ambient_roots.extend(source_roots.iter().cloned());
    ambient_roots.extend(dot_roots.clone());
    dedup_paths(&mut ambient_roots);
    let mut include_roots = source_roots.to_vec();
    include_roots.extend(dot_roots.clone());
    dedup_paths(&mut include_roots);
    let dot_vue_roots = dot_roots.clone();
    let mut compiler_options = serde_json::Map::new();
    compiler_options.insert("ignoreDeprecations".to_string(), json!("6.0"));
    compiler_options.insert(
        "rootDir".to_string(),
        json!(config_relative_path(&config_dir, fixture_root)),
    );
    let path_mapping_root = compiler_paths
        .base_url
        .as_ref()
        .map(|base_url| base_url.dir.join(&base_url.value))
        .or_else(|| compiler_paths.paths.as_ref().map(|paths| paths.dir.clone()));
    let mut paths = compiler_paths
        .paths
        .as_ref()
        .zip(path_mapping_root.as_deref())
        .map(|(paths, path_mapping_root)| {
            source_path_mappings(path_mapping_root, &config_dir, &paths.value)
        })
        .unwrap_or_default();
    extend_local_vue_runtime_paths(fixture_root, &config_dir, &mut paths)?;
    if !paths.is_empty() {
        compiler_options.insert("paths".to_string(), Value::Object(paths));
        if compiler_paths.base_url.is_some() {
            compiler_options.insert("baseUrl".to_string(), json!("."));
        }
    }
    let files = baseline_files(&config_dir, fixture_root, vize_report)?;
    let config = json!({
        "extends": config_relative_path(&config_dir, &source_path),
        "compilerOptions": Value::Object(compiler_options),
        "files": files,
        "include": include_globs(&config_dir, &ambient_roots, &include_roots, &dot_vue_roots),
        "exclude": ambient_roots.iter().flat_map(|root_path| {
            let root = config_relative_path(&config_dir, root_path);
            vec![format!("{root}/**/node_modules/**"), format!("{root}/**/dist/**")]
        }).collect::<Vec<_>>(),
        "references": [],
    });
    let source = format!(
        "{}\n",
        serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?
    );
    common::write_text(&output_path, &source)?;
    common::write_text(&artifact_path, &source)?;
    let _ = root;
    Ok(BaselineConfig {
        path: output_path,
        source,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Diagnostic {
    file: String,
    severity: String,
    line: u64,
    column: u64,
    code: u64,
    message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SharedDiagnostic {
    file: String,
    severity: String,
    line: u64,
    column: u64,
    code: u64,
    vize_message: String,
    baseline_message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiagnosticSide {
    code: u64,
    message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DocumentedDifference {
    project: String,
    file: String,
    severity: String,
    line: u64,
    column: u64,
    vize: Option<DiagnosticSide>,
    baseline: Option<DiagnosticSide>,
    issue: u64,
    reason: String,
}

struct DiagnosticInput {
    diagnostics: Vec<Diagnostic>,
    excluded_non_vue_count: usize,
    excluded_project_count: usize,
    excluded_external_count: usize,
    excluded_support_vue_count: usize,
}

struct ToolRun {
    command: String,
    status: i32,
    stdout: String,
    stderr: String,
    output: String,
    duration_ms: u128,
    parsed: Option<Value>,
}

struct CapturedToolRun {
    run: ToolRun,
    run_error: Option<String>,
}

struct MutationContext<'a> {
    project: &'a Value,
    fixture_root: &'a Path,
    vize_report: &'a Value,
    coverage: &'a Value,
    configuration: &'a Value,
    baseline_config: &'a BaselineConfig,
    vize_bin: &'a Path,
    vue_tsc_bin: &'a Path,
    documented_differences: &'a [DocumentedDifference],
}

struct MutationCandidate {
    seed: String,
    file: String,
    clean_source: String,
    broken_source: String,
    line: u64,
    column: u64,
}

struct ObservedMutation {
    source_sha256: String,
    vize: ToolRun,
    baseline: ToolRun,
    comparison: Value,
}

fn compare_typecheck_diagnostics(
    project_id: String,
    cwd: &Path,
    vize_report: &Value,
    vue_tsc_output: &str,
    source_roots: &[PathBuf],
    documented_differences: &[DocumentedDifference],
) -> Result<Value, String> {
    if project_id.is_empty() {
        invalid_divergence("project id is required")?;
    }
    if !cwd.is_absolute() {
        invalid_divergence("cwd must be absolute")?;
    }
    let expected = select_documented_differences(documented_differences, &project_id, cwd)?;
    let vize_input = collect_vize_diagnostics(vize_report, cwd)?;
    let comparable_vue_files = comparable_vue_file_set(vize_report, cwd)?;
    let support_classifier = SupportVueClassifier::new(cwd, source_roots);
    let baseline_input = collect_vue_tsc_diagnostics(
        vue_tsc_output,
        cwd,
        &comparable_vue_files,
        &support_classifier,
    )?;
    let mut vize_groups = group_by_identity(vize_input.diagnostics);
    let mut baseline_groups = group_by_identity(baseline_input.diagnostics);
    let mut identities = vize_groups
        .keys()
        .chain(baseline_groups.keys())
        .cloned()
        .collect::<Vec<_>>();
    sort_bytes_dedup(&mut identities);

    let mut shared = Vec::new();
    let mut message_mismatches = Vec::new();
    let mut false_positives = Vec::new();
    let mut false_negatives = Vec::new();
    for identity in identities {
        let candidates = vize_groups.remove(&identity).unwrap_or_default();
        let expected = baseline_groups.remove(&identity).unwrap_or_default();
        let common_count = candidates.len().min(expected.len());
        for index in 0..common_count {
            let candidate = &candidates[index];
            let baseline = &expected[index];
            let pair = SharedDiagnostic {
                file: candidate.file.clone(),
                severity: candidate.severity.clone(),
                line: candidate.line,
                column: candidate.column,
                code: candidate.code,
                vize_message: candidate.message.clone(),
                baseline_message: baseline.message.clone(),
            };
            if pair.vize_message == pair.baseline_message {
                shared.push(pair);
            } else {
                message_mismatches.push(pair);
            }
        }
        false_positives.extend(candidates.into_iter().skip(common_count));
        false_negatives.extend(expected.into_iter().skip(common_count));
    }
    shared.sort_by(compare_shared);
    message_mismatches.sort_by(compare_shared);
    false_positives.sort_by(compare_diagnostics);
    false_negatives.sort_by(compare_diagnostics);
    let documented = pair_documented_differences(
        &expected,
        &mut false_positives,
        &mut false_negatives,
        &mut message_mismatches,
    );
    let documented_vize_count = documented
        .iter()
        .filter(|difference| difference.vize.is_some())
        .count();
    let documented_baseline_count = documented
        .iter()
        .filter(|difference| difference.baseline.is_some())
        .count();
    let vize_count =
        shared.len() + message_mismatches.len() + documented_vize_count + false_positives.len();
    let baseline_count = shared.len()
        + message_mismatches.len()
        + documented_baseline_count
        + false_negatives.len();
    let summary = json!({
        "vizeDiagnosticCount": vize_count,
        "baselineDiagnosticCount": baseline_count,
        "sharedCount": shared.len(),
        "messageMismatchCount": message_mismatches.len(),
        "documentedDifferenceCount": documented.len(),
        "falsePositiveCount": false_positives.len(),
        "falseNegativeCount": false_negatives.len(),
        "falsePositiveRatio": ratio(false_positives.len(), vize_count),
        "falseNegativeRatio": ratio(false_negatives.len(), baseline_count),
        "vizeExcludedNonVueCount": vize_input.excluded_non_vue_count,
        "baselineExcludedNonVueCount": baseline_input.excluded_non_vue_count,
        "baselineExcludedProjectCount": baseline_input.excluded_project_count,
        "baselineExcludedExternalCount": baseline_input.excluded_external_count,
        "baselineExcludedSupportVueCount": baseline_input.excluded_support_vue_count,
    });
    let shared_values = shared.iter().map(shared_json).collect::<Vec<_>>();
    let mismatch_values = message_mismatches
        .iter()
        .map(shared_json)
        .collect::<Vec<_>>();
    let false_positive_values = false_positives
        .iter()
        .map(diagnostic_json)
        .collect::<Vec<_>>();
    let false_negative_values = false_negatives
        .iter()
        .map(diagnostic_json)
        .collect::<Vec<_>>();
    let documented_values = documented.iter().map(documented_json).collect::<Vec<_>>();
    let digest_payload = json!({
        "summary": summary,
        "shared": shared_values,
        "messageMismatches": mismatch_values,
        "falsePositives": false_positive_values,
        "falseNegatives": false_negative_values,
        "documentedDifferences": documented_values,
    });
    Ok(json!({
        "schema": "vize.fixtureTypecheckDivergence",
        "version": 4,
        "project": project_id,
        "summary": digest_payload["summary"],
        "shared": digest_payload["shared"],
        "messageMismatches": digest_payload["messageMismatches"],
        "falsePositives": digest_payload["falsePositives"],
        "falseNegatives": digest_payload["falseNegatives"],
        "documentedDifferences": digest_payload["documentedDifferences"],
        "sha256": sha256(&serde_json::to_string(&digest_payload).map_err(|error| error.to_string())?),
    }))
}

fn collect_vize_diagnostics(report: &Value, cwd: &Path) -> Result<DiagnosticInput, String> {
    let files = report
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| divergence_error("Vize report must contain files"))?;
    let pattern = Regex::new(r"^(error|warning):(\d+):(\d+) \[TS(\d+)\] ([\s\S]+)$").unwrap();
    let mut diagnostics = Vec::new();
    for (file_index, file) in files.iter().enumerate() {
        let entries = file
            .get("diagnostics")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                divergence_error(&format!(
                    "Vize files[{file_index}] must contain diagnostics"
                ))
            })?;
        let normalized_file = normalize_vize_path(
            file.get("file").and_then(Value::as_str),
            cwd,
            &format!("Vize files[{file_index}].file"),
        )?;
        for (diagnostic_index, diagnostic) in entries.iter().enumerate() {
            let diagnostic = diagnostic.as_str().ok_or_else(|| {
                divergence_error(&format!(
                    "Vize diagnostic {file_index}:{diagnostic_index} must be a string"
                ))
            })?;
            let captures = pattern.captures(diagnostic).ok_or_else(|| {
                divergence_error(&format!("unparseable Vize diagnostic {normalized_file}"))
            })?;
            diagnostics.push(record(
                normalized_file.clone(),
                captures[1].to_string(),
                &captures[2],
                &captures[3],
                &captures[4],
                &captures[5],
            )?);
        }
    }
    let included = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.file.ends_with(".vue"))
        .cloned()
        .collect::<Vec<_>>();
    Ok(DiagnosticInput {
        excluded_non_vue_count: diagnostics.len() - included.len(),
        diagnostics: included,
        excluded_project_count: 0,
        excluded_external_count: 0,
        excluded_support_vue_count: 0,
    })
}

fn collect_vue_tsc_diagnostics(
    output: &str,
    cwd: &Path,
    comparable_vue_files: &BTreeSet<String>,
    support_classifier: &SupportVueClassifier,
) -> Result<DiagnosticInput, String> {
    let positioned = Regex::new(r"^(.+)\((\d+),(\d+)\): (error|warning) TS(\d+): (.+)$").unwrap();
    let project = Regex::new(r"^(error|warning) TS(\d+): (.+)$").unwrap();
    let mut diagnostics = Vec::new();
    let mut excluded_non_vue_count = 0;
    let mut excluded_project_count = 0;
    let mut excluded_external_count = 0;
    let mut excluded_support_vue_count = 0;
    for raw_line in output.replace("\r\n", "\n").split('\n') {
        let line = raw_line.trim_end();
        if let Some(captures) = positioned.captures(line) {
            match normalize_baseline_path(&captures[1], cwd)? {
                None => excluded_external_count += 1,
                Some(file) if !file.ends_with(".vue") => excluded_non_vue_count += 1,
                Some(file)
                    if !comparable_vue_files.contains(&file)
                        && support_classifier.is_support_vue_file(&file) =>
                {
                    excluded_support_vue_count += 1
                }
                Some(file) => diagnostics.push(record(
                    file,
                    captures[4].to_string(),
                    &captures[2],
                    &captures[3],
                    &captures[5],
                    &captures[6],
                )?),
            }
            continue;
        }
        if let Some(captures) = project.captures(line) {
            let _ = record(
                "<project>".to_string(),
                captures[1].to_string(),
                "1",
                "1",
                &captures[2],
                &captures[3],
            )?;
            excluded_project_count += 1;
            continue;
        }
        if Regex::new(r"\b(?:error|warning) TS\d+:")
            .unwrap()
            .is_match(line)
        {
            invalid_divergence(&format!("unparseable vue-tsc diagnostic: {line}"))?;
        }
    }
    Ok(DiagnosticInput {
        diagnostics,
        excluded_non_vue_count,
        excluded_project_count,
        excluded_external_count,
        excluded_support_vue_count,
    })
}

fn evaluate_vue_program_coverage(
    vize_report: &Value,
    vue_tsc_output: &str,
    cwd: &Path,
    source_roots: &[PathBuf],
) -> Result<Value, String> {
    let mut vize_vue_files = vize_report
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(
            vize_report
                .get("fileCount")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        )
        .enumerate()
        .map(|(index, entry)| {
            normalize_vize_path(
                entry.get("file").and_then(Value::as_str),
                cwd,
                &format!("Vize files[{index}].file"),
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|file| file.ends_with(".vue"))
        .collect::<Vec<_>>();
    sort_bytes_dedup(&mut vize_vue_files);
    let comparable = vize_vue_files.iter().cloned().collect::<BTreeSet<_>>();
    let support_classifier = SupportVueClassifier::new(cwd, source_roots);
    let (baseline_vue_files, dependency_vue_files, support_vue_files) =
        collect_vue_tsc_program_files(vue_tsc_output, cwd, &comparable, &support_classifier)?;
    let baseline_set = baseline_vue_files.iter().cloned().collect::<BTreeSet<_>>();
    let vize_set = vize_vue_files.iter().cloned().collect::<BTreeSet<_>>();
    let missing_vue_files = vize_vue_files
        .iter()
        .filter(|file| !baseline_set.contains(*file))
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_vue_files = baseline_vue_files
        .iter()
        .filter(|file| !vize_set.contains(*file))
        .cloned()
        .collect::<Vec<_>>();
    let shared_vue_file_count = vize_vue_files.len() - missing_vue_files.len();
    let unusable_reason = if missing_vue_files.is_empty() && unexpected_vue_files.is_empty() {
        Value::Null
    } else {
        json!(format!(
            "vue-tsc checked {} Vue files while Vize checked {} (missing {}, unexpected {})",
            baseline_vue_files.len(),
            vize_vue_files.len(),
            missing_vue_files.len(),
            unexpected_vue_files.len()
        ))
    };
    Ok(json!({
        "baselineVueFileCount": baseline_vue_files.len(),
        "baselineVueFilesSha256": file_list_hash(&baseline_vue_files),
        "ignoredDependencyVueFileCount": dependency_vue_files.len(),
        "ignoredDependencyVueFilesSha256": file_list_hash(&dependency_vue_files),
        "ignoredSupportVueFileCount": support_vue_files.len(),
        "ignoredSupportVueFilesSha256": file_list_hash(&support_vue_files),
        "missingVueFiles": missing_vue_files,
        "sharedVueFileCount": shared_vue_file_count,
        "unexpectedVueFiles": unexpected_vue_files,
        "unusableReason": unusable_reason,
        "verdict": if missing_vue_files.is_empty() && unexpected_vue_files.is_empty() { "usable" } else { "unusable" },
        "vizeVueFileCount": vize_vue_files.len(),
        "vizeVueFilesSha256": file_list_hash(&vize_vue_files),
    }))
}

fn unavailable_vue_program_coverage(
    vize_report: &Value,
    cwd: &Path,
    reason: &str,
) -> Result<Value, String> {
    let mut vize_vue_files = vize_report
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(
            vize_report
                .get("fileCount")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        )
        .enumerate()
        .map(|(index, entry)| {
            normalize_vize_path(
                entry.get("file").and_then(Value::as_str),
                cwd,
                &format!("Vize files[{index}].file"),
            )
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|file| file.ends_with(".vue"))
        .collect::<Vec<_>>();
    sort_bytes_dedup(&mut vize_vue_files);
    Ok(json!({
        "baselineVueFileCount": 0,
        "baselineVueFilesSha256": file_list_hash(&[]),
        "ignoredDependencyVueFileCount": 0,
        "ignoredDependencyVueFilesSha256": file_list_hash(&[]),
        "ignoredSupportVueFileCount": 0,
        "ignoredSupportVueFilesSha256": file_list_hash(&[]),
        "missingVueFiles": vize_vue_files,
        "sharedVueFileCount": 0,
        "unexpectedVueFiles": [],
        "unusableReason": reason,
        "verdict": "unusable",
        "vizeVueFileCount": vize_vue_files.len(),
        "vizeVueFilesSha256": file_list_hash(&vize_vue_files),
    }))
}

fn collect_vue_tsc_program_files(
    output: &str,
    cwd: &Path,
    comparable_vue_files: &BTreeSet<String>,
    support_classifier: &SupportVueClassifier,
) -> Result<(Vec<String>, Vec<String>, Vec<String>), String> {
    let mut files = Vec::new();
    let mut dependency_files = Vec::new();
    let mut support_files = Vec::new();
    for raw_line in output.replace("\r\n", "\n").split('\n') {
        let line = raw_line.trim_end().replace('\\', "/");
        if !line.ends_with(".vue") || !is_absolute_program_path(&line) {
            continue;
        }
        let Some(relative) = pathdiff::diff_paths(Path::new(&line), cwd) else {
            continue;
        };
        let file = common::normalize_path(&relative);
        let segments = file.split('/').collect::<Vec<_>>();
        if file.is_empty()
            || is_absolute_program_path(&file)
            || segments
                .iter()
                .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
        {
            continue;
        }
        if segments.contains(&"node_modules") {
            dependency_files.push(file);
        } else if !comparable_vue_files.contains(&file)
            && support_classifier.is_support_vue_file(&file)
        {
            support_files.push(file);
        } else {
            files.push(file);
        }
    }
    sort_bytes_dedup(&mut files);
    sort_bytes_dedup(&mut dependency_files);
    sort_bytes_dedup(&mut support_files);
    Ok((files, dependency_files, support_files))
}

fn evaluate_baseline_configuration(output: &str) -> Result<Value, String> {
    let positioned = Regex::new(r"^(.+)\((\d+),(\d+)\): (error|warning) TS(\d+): (.+)$").unwrap();
    let project = Regex::new(r"^(error|warning) TS(\d+): (.+)$").unwrap();
    let configuration_file = Regex::new(r"[jt]sconfig[^/]*\.json$").unwrap();
    let mut diagnostics = Vec::new();
    for raw_line in output.replace("\r\n", "\n").split('\n') {
        let line = raw_line.trim_end();
        if let Some(captures) = positioned.captures(line) {
            let file = captures[1].replace('\\', "/");
            let code = captures[5]
                .parse::<u64>()
                .map_err(|_| "invalid TypeScript diagnostic code".to_string())?;
            if !configuration_file.is_match(&file) && code != 6059 && code != 6307 {
                continue;
            }
            diagnostics.push(json!({
                "code": code,
                "column": captures[3].parse::<u64>().map_err(|_| "invalid diagnostic column".to_string())?,
                "file": file,
                "line": captures[2].parse::<u64>().map_err(|_| "invalid diagnostic line".to_string())?,
                "message": normalize_message(&captures[6]),
                "severity": captures[4].to_string(),
            }));
            continue;
        }
        if let Some(captures) = project.captures(line) {
            diagnostics.push(json!({
                "code": captures[2].parse::<u64>().map_err(|_| "invalid TypeScript diagnostic code".to_string())?,
                "column": Value::Null,
                "file": Value::Null,
                "line": Value::Null,
                "message": normalize_message(&captures[3]),
                "severity": captures[1].to_string(),
            }));
        }
    }
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.get("severity").and_then(Value::as_str) == Some("error"))
        .collect::<Vec<_>>();
    let unusable_reason = if errors.is_empty() {
        Value::Null
    } else {
        json!(format!(
            "vue-tsc could not load the fixture project configuration ({} error{}): {}",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" },
            render_configuration_diagnostic(errors[0])
        ))
    };
    Ok(json!({
        "diagnostics": diagnostics,
        "errorCount": errors.len(),
        "unusableReason": unusable_reason,
        "verdict": if errors.is_empty() { "usable" } else { "unusable" },
    }))
}

fn unavailable_baseline_configuration(reason: &str) -> Value {
    json!({
        "diagnostics": [],
        "errorCount": 0,
        "unusableReason": reason,
        "verdict": "unusable",
    })
}

fn evaluate_baseline_ambient_environment(
    output: &str,
    fixture_root: &Path,
) -> Result<Value, String> {
    let fixture_prefix = format!("{}/", normalize_display_path(fixture_root));
    let mut roots: HashMap<String, HashMap<String, bool>> = HashMap::new();
    let mut external_file_count = 0usize;
    for raw_line in output.replace("\r\n", "\n").split('\n') {
        let file = raw_line.trim_end().replace('\\', "/");
        if !is_absolute_program_path(&file) {
            continue;
        }
        let inside = file.starts_with(&fixture_prefix);
        if !inside {
            external_file_count += 1;
        }
        let Some((name, root)) = parse_package_root(&file) else {
            continue;
        };
        roots.entry(name).or_default().insert(root, inside);
    }
    let runtime_names = ["@vue/runtime-core", "@vue/runtime-dom", "vue"];
    let mut vue_runtime = Vec::new();
    for name in runtime_names {
        if let Some(copies) = roots.get(name) {
            vue_runtime.push(describe_package(name, copies, fixture_root));
        }
    }
    let mut external_packages = roots
        .iter()
        .filter(|(_, copies)| copies.values().any(|inside| !*inside))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    external_packages.sort();
    let unusable_reason = first_ambient_failure(&vue_runtime);
    Ok(json!({
        "externalFileCount": external_file_count,
        "externalPackages": external_packages,
        "unusableReason": unusable_reason.clone().map(Value::String).unwrap_or(Value::Null),
        "verdict": if unusable_reason.is_some() { "contaminated" } else { "isolated" },
        "vueRuntime": vue_runtime,
    }))
}

fn unavailable_baseline_ambient_environment(reason: &str) -> Value {
    json!({
        "externalFileCount": 0,
        "externalPackages": [],
        "unusableReason": reason,
        "verdict": "unavailable",
        "vueRuntime": [],
    })
}

fn create_seeded_mutation_oracle(ctx: MutationContext<'_>) -> Result<Value, String> {
    let project_id = project_string(ctx.project, "id")?;
    let revision = project_string(ctx.project, "revision")?;
    let seed = sha256(
        &[
            project_id.as_str(),
            revision.as_str(),
            ctx.coverage
                .get("vizeVueFilesSha256")
                .and_then(Value::as_str)
                .unwrap_or(""),
            ctx.coverage
                .get("baselineVueFilesSha256")
                .and_then(Value::as_str)
                .unwrap_or(""),
        ]
        .join("\0"),
    );
    if ctx.coverage.get("verdict").and_then(Value::as_str) != Some("usable") {
        return Ok(unusable_mutation(
            &seed,
            ctx.coverage
                .get("unusableReason")
                .and_then(Value::as_str)
                .unwrap_or("Vue corpus coverage is unusable"),
        ));
    }
    if ctx.configuration.get("verdict").and_then(Value::as_str) == Some("unusable") {
        return Ok(unusable_mutation(
            &seed,
            ctx.configuration
                .get("unusableReason")
                .and_then(Value::as_str)
                .unwrap_or("vue-tsc project configuration is unusable"),
        ));
    }
    if optional_typecheck_corpus_globs(ctx.project).is_none() {
        return Ok(unusable_mutation(
            &seed,
            "seeded mutation requires configured typecheck corpus globs to rerun Vize",
        ));
    }
    let mut shared_files = ctx
        .vize_report
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(
            ctx.vize_report
                .get("fileCount")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        )
        .filter_map(|entry| entry.get("file").and_then(Value::as_str))
        .filter(|file| file.ends_with(".vue"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    sort_bytes_dedup(&mut shared_files);
    let shared_count = ctx
        .coverage
        .get("sharedVueFileCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let vize_count = ctx
        .coverage
        .get("vizeVueFileCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let baseline_count = ctx
        .coverage
        .get("baselineVueFileCount")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    if shared_files.is_empty()
        || shared_files.len() != shared_count
        || shared_files.len() != vize_count
        || shared_files.len() != baseline_count
    {
        return Ok(unusable_mutation(
            &seed,
            &format!(
                "seeded mutation requires a non-empty shared authored Vue corpus, got {} shared file(s)",
                shared_files.len()
            ),
        ));
    }
    let start = usize::from_str_radix(&seed[..8], 16).unwrap_or(0) % shared_files.len();
    let vize_diagnostic_counts = vize_report_diagnostic_counts(ctx.vize_report);
    let mut candidates = Vec::new();
    let mut failed_oracle = None;
    for offset in 0..shared_files.len() {
        let file = shared_files[(start + offset) % shared_files.len()].clone();
        let clean_source = common::read_text(ctx.fixture_root.join(&file))?;
        let Some((broken_source, line, column)) = build_seeded_mutation(&clean_source) else {
            continue;
        };
        let candidate = MutationCandidate {
            seed: seed.clone(),
            file,
            clean_source,
            broken_source,
            line,
            column,
        };
        candidates.push((offset, candidate));
    }
    candidates.sort_by_key(|(offset, candidate)| {
        (
            vize_diagnostic_counts
                .get(candidate.file.as_str())
                .copied()
                .unwrap_or(usize::MAX),
            candidate.clean_source.len(),
            *offset,
        )
    });
    for (_, candidate) in candidates {
        let oracle = observe_mutation_candidate(&ctx, &candidate)?;
        if oracle.get("passed").and_then(Value::as_bool) == Some(true) {
            return Ok(oracle);
        }
        failed_oracle = Some(oracle);
    }
    if failed_oracle.is_none() {
        return Ok(unusable_mutation(
            &seed,
            "seeded mutation found no authored Vue file accepting a TS probe",
        ));
    }
    Ok(failed_oracle.unwrap_or_else(|| {
        unusable_mutation(
            &seed,
            "seeded mutation found no authored Vue file accepting a TS probe",
        )
    }))
}

fn vize_report_diagnostic_counts(report: &Value) -> HashMap<String, usize> {
    let Some(files) = report.get("files").and_then(Value::as_array) else {
        return HashMap::new();
    };
    files
        .iter()
        .filter_map(|entry| {
            Some((
                entry.get("file")?.as_str()?.to_string(),
                entry
                    .get("diagnostics")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            ))
        })
        .collect()
}

fn observe_mutation_candidate(
    ctx: &MutationContext<'_>,
    candidate: &MutationCandidate,
) -> Result<Value, String> {
    let source_path = ctx.fixture_root.join(&candidate.file);
    let diagnostic = Diagnostic {
        file: candidate.file.clone(),
        severity: "error".to_string(),
        line: candidate.line,
        column: candidate.column,
        code: 2322,
        message: "Type 'number' is not assignable to type 'string'.".to_string(),
    };
    let result = (|| -> Result<Value, String> {
        common::write_text(&source_path, &candidate.clean_source)?;
        let clean = observe_mutation_state(ctx, candidate, "clean", &source_path)?;
        common::write_text(&source_path, &candidate.broken_source)?;
        let broken = observe_mutation_state(ctx, candidate, "broken", &source_path)?;
        common::write_text(&source_path, &candidate.clean_source)?;
        let repaired = observe_mutation_state(ctx, candidate, "repaired", &source_path)?;
        summarize_mutation_observations(candidate, &diagnostic, &clean, &broken, &repaired)
    })();
    let restore_result = common::write_text(&source_path, &candidate.clean_source);
    match (result, restore_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Ok(unusable_mutation(
            &candidate.seed,
            &format!("seeded mutation oracle could not run real typecheckers: {error}"),
        )),
        (Ok(_), Err(error)) => Err(format!(
            "Seeded mutation oracle could not restore {}: {error}",
            source_path.display()
        )),
        (Err(primary), Err(error)) => Err(format!(
            "{primary}; Seeded mutation oracle could not restore {}: {error}",
            source_path.display()
        )),
    }
}

fn observe_mutation_state(
    ctx: &MutationContext<'_>,
    candidate: &MutationCandidate,
    name: &str,
    source_path: &Path,
) -> Result<ObservedMutation, String> {
    let source_bytes = fs::read(source_path)
        .map_err(|error| format!("cannot read {}: {error}", source_path.display()))?;
    let source_sha256 = sha256_bytes(&source_bytes);
    let vize = run_vize_typecheck(ctx, candidate)?;
    assert_source_unchanged(name, &candidate.file, source_path, &source_sha256, "Vize")?;
    let baseline = run_vue_tsc_mutation(ctx, candidate)?;
    assert_source_unchanged(
        name,
        &candidate.file,
        source_path,
        &source_sha256,
        "vue-tsc",
    )?;
    let parsed = vize
        .parsed
        .as_ref()
        .ok_or_else(|| "Vize mutation run emitted no parsed JSON".to_string())?;
    let source_roots = source_roots(ctx.fixture_root, ctx.project, parsed)?;
    let comparison = compare_typecheck_diagnostics(
        project_string(ctx.project, "id")?,
        ctx.fixture_root,
        parsed,
        &baseline.output,
        &source_roots,
        ctx.documented_differences,
    )?;
    Ok(ObservedMutation {
        source_sha256,
        vize,
        baseline,
        comparison,
    })
}

fn run_vize_typecheck(
    ctx: &MutationContext<'_>,
    candidate: &MutationCandidate,
) -> Result<ToolRun, String> {
    let args = mutation_tool_args(ctx.project, candidate);
    let mut run = run_capture_limited(
        ctx.vize_bin,
        &args,
        ctx.fixture_root,
        ctx.project
            .pointer("/typecheckPerformance/hangTimeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(5_000),
        "Vize mutation run",
        &[0, 1],
    )
    .map_err(mutation_run_error)?;
    let parsed = serde_json::from_str::<Value>(&run.stdout)
        .map_err(|error| format!("Vize mutation run emitted invalid JSON: {error}"))?;
    let expected_files = vec![candidate.file.clone()];
    let authored_files = collect_typechecker_authored_paths(ctx.fixture_root)?;
    validate_typechecker_output(
        ctx.project,
        &parsed,
        run.status,
        Some(&expected_files),
        Some(&authored_files),
    )?;
    run.parsed = Some(parsed);
    let _ = candidate;
    Ok(run)
}

fn run_vue_tsc_mutation(
    ctx: &MutationContext<'_>,
    candidate: &MutationCandidate,
) -> Result<ToolRun, String> {
    let args = mutation_baseline_args(ctx, candidate)?;
    run_capture_limited(
        ctx.vue_tsc_bin,
        &args,
        ctx.fixture_root,
        ctx.project
            .pointer("/typecheckPerformance/hangTimeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(5_000),
        "vue-tsc mutation run",
        &[0, 1, 2],
    )
    .map_err(mutation_run_error)
}

fn mutation_tool_args(project: &Value, candidate: &MutationCandidate) -> Vec<String> {
    // The full matrix run already proved corpus coverage. Mutation probes only
    // need the selected source file, and large fixtures can otherwise spend the
    // release budget on repeated whole-project checks.
    let mut args = vec![
        "check".to_string(),
        candidate.file.clone(),
        "--format".to_string(),
        "json".to_string(),
        "--no-config".to_string(),
    ];
    if let Some(tsconfig) = typecheck_tsconfig_path(project) {
        args.extend(["--tsconfig".to_string(), tsconfig]);
    }
    args
}

fn mutation_baseline_args(
    ctx: &MutationContext<'_>,
    candidate: &MutationCandidate,
) -> Result<Vec<String>, String> {
    let config_dir = ctx
        .baseline_config
        .path
        .parent()
        .ok_or_else(|| "baseline config path has no parent".to_string())?;
    let project_id = project_string(ctx.project, "id")?;
    let config_path = config_dir.join(format!("{project_id}-mutation-vue-tsc.tsconfig.json"));
    let mut config = serde_json::from_str::<Value>(&ctx.baseline_config.source)
        .map_err(|error| format!("baseline config source is invalid JSON: {error}"))?;
    config["files"] = json!([config_relative_path(
        config_dir,
        &ctx.fixture_root.join(&candidate.file),
    )]);
    common::write_text(
        &config_path,
        &format!(
            "{}\n",
            serde_json::to_string_pretty(&config).map_err(|error| error.to_string())?
        ),
    )?;
    Ok(vec![
        "--noEmit".to_string(),
        "--pretty".to_string(),
        "false".to_string(),
        "-p".to_string(),
        config_path.display().to_string(),
    ])
}

fn summarize_mutation_observations(
    candidate: &MutationCandidate,
    diagnostic: &Diagnostic,
    clean: &ObservedMutation,
    broken: &ObservedMutation,
    repaired: &ObservedMutation,
) -> Result<Value, String> {
    let broken_delta = comparison_delta(&clean.comparison, &broken.comparison);
    let repaired_delta = comparison_delta(&clean.comparison, &repaired.comparison);
    let clean_expected = comparison_has_diagnostic(&clean.comparison, diagnostic);
    let expected_matched = broken_delta
        .get("shared")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .any(|record| matches_seeded_probe(record, diagnostic));
    let repaired_expected = comparison_has_diagnostic(&repaired.comparison, diagnostic);
    let states = vec![
        mutation_state_json("clean", &candidate.clean_source, clean, &empty_delta())?,
        mutation_state_json(
            "broken",
            &candidate.broken_source,
            broken,
            &broken_delta["summary"],
        )?,
        mutation_state_json(
            "repaired",
            &candidate.clean_source,
            repaired,
            &repaired_delta["summary"],
        )?,
    ];
    let passed = !clean_expected
        && states[1].get("sharedCount").and_then(Value::as_u64) == Some(1)
        && states[1]
            .get("messageMismatchCount")
            .and_then(Value::as_u64)
            == Some(0)
        && states[1]
            .get("documentedDifferenceCount")
            .and_then(Value::as_u64)
            == Some(0)
        && states[1].get("falsePositiveCount").and_then(Value::as_u64) == Some(0)
        && states[1].get("falseNegativeCount").and_then(Value::as_u64) == Some(0)
        && expected_matched
        && states[2].get("sourceSha256") == states[0].get("sourceSha256")
        && states[2].get("sharedCount").and_then(Value::as_u64) == Some(0)
        && states[2]
            .get("messageMismatchCount")
            .and_then(Value::as_u64)
            == Some(0)
        && states[2]
            .get("documentedDifferenceCount")
            .and_then(Value::as_u64)
            == Some(0)
        && states[2].get("falsePositiveCount").and_then(Value::as_u64) == Some(0)
        && states[2].get("falseNegativeCount").and_then(Value::as_u64) == Some(0)
        && !repaired_expected;
    let documented_differences = observed_documented_differences(&[
        &clean.comparison,
        &broken.comparison,
        &repaired.comparison,
    ]);
    Ok(json!({
        "schema": "vize.fixtureTypecheckSeededMutationOracle",
        "version": 1,
        "seed": candidate.seed,
        "verdict": if passed { "passed" } else { "unusable" },
        "passed": passed,
        "unusableReason": if passed { Value::Null } else { json!("seeded mutation oracle did not produce one shared broken diagnostic and clean repair") },
        "file": candidate.file,
        "sourceSha256": sha256(&candidate.clean_source),
        "span": { "line": candidate.line, "column": candidate.column },
        "diagnostic": diagnostic_json(diagnostic),
        "cleanExpectedDiagnosticPresent": clean_expected,
        "expectedDiagnosticMatched": expected_matched,
        "repairedExpectedDiagnosticPresent": repaired_expected,
        "documentedDifferences": documented_differences,
        "states": states,
    }))
}

fn observed_documented_differences(comparisons: &[&Value]) -> Vec<Value> {
    let mut observed = BTreeMap::new();
    for comparison in comparisons {
        for difference in comparison
            .get("documentedDifferences")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
        {
            observed.insert(documented_key(difference), difference.clone());
        }
    }
    observed.into_values().collect()
}

fn mutation_state_json(
    name: &str,
    source: &str,
    observed: &ObservedMutation,
    delta: &Value,
) -> Result<Value, String> {
    let planned_sha256 = sha256(source);
    if observed.source_sha256 != planned_sha256 {
        return Err(format!(
            "Seeded {name} oracle source digest {} does not match the planned source digest {planned_sha256}",
            observed.source_sha256
        ));
    }
    let summary = observed
        .comparison
        .get("summary")
        .ok_or_else(|| "mutation comparison is missing summary".to_string())?;
    Ok(json!({
        "name": name,
        "sourceSha256": observed.source_sha256,
        "vizeDiagnosticCount": delta.get("vizeDiagnosticCount").and_then(Value::as_u64).unwrap_or(0),
        "baselineDiagnosticCount": delta.get("baselineDiagnosticCount").and_then(Value::as_u64).unwrap_or(0),
        "sharedCount": delta.get("sharedCount").and_then(Value::as_u64).unwrap_or(0),
        "messageMismatchCount": delta.get("messageMismatchCount").and_then(Value::as_u64).unwrap_or(0),
        "documentedDifferenceCount": delta.get("documentedDifferenceCount").and_then(Value::as_u64).unwrap_or(0),
        "falsePositiveCount": delta.get("falsePositiveCount").and_then(Value::as_u64).unwrap_or(0),
        "falseNegativeCount": delta.get("falseNegativeCount").and_then(Value::as_u64).unwrap_or(0),
        "observed": summary_evidence(summary),
        "vize": run_evidence(&observed.vize),
        "baseline": run_evidence(&observed.baseline),
    }))
}

fn comparison_delta(clean: &Value, current: &Value) -> Value {
    let shared = subtract_records(
        current
            .get("shared")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        clean
            .get("shared")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        shared_key,
    );
    let message_mismatches = subtract_records(
        current
            .get("messageMismatches")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        clean
            .get("messageMismatches")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        shared_key,
    );
    let documented_differences = subtract_records(
        current
            .get("documentedDifferences")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        clean
            .get("documentedDifferences")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        documented_key,
    );
    let false_positives = subtract_records(
        current
            .get("falsePositives")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        clean
            .get("falsePositives")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        diagnostic_key,
    );
    let false_negatives = subtract_records(
        current
            .get("falseNegatives")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        clean
            .get("falseNegatives")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new()),
        diagnostic_key,
    );
    let documented_vize_count = documented_differences
        .iter()
        .filter(|difference| documented_side_is_present(difference, "vize"))
        .count();
    let documented_baseline_count = documented_differences
        .iter()
        .filter(|difference| documented_side_is_present(difference, "baseline"))
        .count();
    json!({
        "shared": shared,
        "summary": {
            "vizeDiagnosticCount": shared.len() + message_mismatches.len() + documented_vize_count + false_positives.len(),
            "baselineDiagnosticCount": shared.len() + message_mismatches.len() + documented_baseline_count + false_negatives.len(),
            "sharedCount": shared.len(),
            "messageMismatchCount": message_mismatches.len(),
            "documentedDifferenceCount": documented_differences.len(),
            "falsePositiveCount": false_positives.len(),
            "falseNegativeCount": false_negatives.len(),
        }
    })
}

fn documented_side_is_present(difference: &Value, side: &str) -> bool {
    difference.get(side).is_some_and(|value| !value.is_null())
}

fn subtract_records(
    current: &[Value],
    clean: &[Value],
    key_of: fn(&Value) -> String,
) -> Vec<Value> {
    let mut remaining: HashMap<String, usize> = HashMap::new();
    for record in clean {
        *remaining.entry(key_of(record)).or_default() += 1;
    }
    current
        .iter()
        .filter_map(|record| {
            let key = key_of(record);
            let count = remaining.get(&key).copied().unwrap_or(0);
            if count == 0 {
                Some(record.clone())
            } else {
                remaining.insert(key, count - 1);
                None
            }
        })
        .collect()
}

fn empty_delta() -> Value {
    json!({
        "vizeDiagnosticCount": 0,
        "baselineDiagnosticCount": 0,
        "sharedCount": 0,
        "messageMismatchCount": 0,
        "documentedDifferenceCount": 0,
        "falsePositiveCount": 0,
        "falseNegativeCount": 0,
    })
}

fn group_by_identity(values: Vec<Diagnostic>) -> BTreeMap<String, Vec<Diagnostic>> {
    let mut groups: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
    for value in values {
        groups.entry(identity_key(&value)).or_default().push(value);
    }
    for group in groups.values_mut() {
        group.sort_by(compare_diagnostics);
    }
    groups
}

fn identity_key(record: &Diagnostic) -> String {
    [
        record.file.as_str(),
        record.severity.as_str(),
        &record.line.to_string(),
        &record.column.to_string(),
        &record.code.to_string(),
    ]
    .join("\0")
}

fn compare_diagnostics(left: &Diagnostic, right: &Diagnostic) -> std::cmp::Ordering {
    compare_bytes(&left.file, &right.file)
        .then_with(|| compare_bytes(&left.severity, &right.severity))
        .then_with(|| left.line.cmp(&right.line))
        .then_with(|| left.column.cmp(&right.column))
        .then_with(|| left.code.cmp(&right.code))
        .then_with(|| compare_bytes(&left.message, &right.message))
}

fn compare_shared(left: &SharedDiagnostic, right: &SharedDiagnostic) -> std::cmp::Ordering {
    compare_bytes(&left.file, &right.file)
        .then_with(|| compare_bytes(&left.severity, &right.severity))
        .then_with(|| left.line.cmp(&right.line))
        .then_with(|| left.column.cmp(&right.column))
        .then_with(|| left.code.cmp(&right.code))
        .then_with(|| compare_bytes(&left.vize_message, &right.vize_message))
        .then_with(|| compare_bytes(&left.baseline_message, &right.baseline_message))
}

fn compare_documented(
    left: &DocumentedDifference,
    right: &DocumentedDifference,
) -> std::cmp::Ordering {
    compare_bytes(&left.file, &right.file)
        .then_with(|| compare_bytes(&left.severity, &right.severity))
        .then_with(|| left.line.cmp(&right.line))
        .then_with(|| left.column.cmp(&right.column))
        .then_with(|| compare_optional_side(left.vize.as_ref(), right.vize.as_ref()))
        .then_with(|| compare_optional_side(left.baseline.as_ref(), right.baseline.as_ref()))
}

fn compare_optional_side(
    left: Option<&DiagnosticSide>,
    right: Option<&DiagnosticSide>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left
            .code
            .cmp(&right.code)
            .then_with(|| compare_bytes(&left.message, &right.message)),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_bytes(left: &str, right: &str) -> std::cmp::Ordering {
    left.as_bytes().cmp(right.as_bytes())
}

fn sort_bytes_dedup(values: &mut Vec<String>) {
    values.sort_by(|left, right| compare_bytes(left, right));
    values.dedup();
}

fn diagnostic_json(record: &Diagnostic) -> Value {
    json!({
        "file": record.file,
        "severity": record.severity,
        "line": record.line,
        "column": record.column,
        "code": record.code,
        "message": record.message,
    })
}

fn shared_json(record: &SharedDiagnostic) -> Value {
    json!({
        "file": record.file,
        "severity": record.severity,
        "line": record.line,
        "column": record.column,
        "code": record.code,
        "vizeMessage": record.vize_message,
        "baselineMessage": record.baseline_message,
    })
}

fn documented_json(record: &DocumentedDifference) -> Value {
    json!({
        "project": record.project,
        "file": record.file,
        "severity": record.severity,
        "line": record.line,
        "column": record.column,
        "vize": record.vize.as_ref().map(|side| json!({ "code": side.code, "message": side.message })).unwrap_or(Value::Null),
        "baseline": record.baseline.as_ref().map(|side| json!({ "code": side.code, "message": side.message })).unwrap_or(Value::Null),
        "issue": record.issue,
        "reason": record.reason,
    })
}

fn reject_stale_documented_differences(
    project_id: &str,
    expected: &[DocumentedDifference],
    divergence: &Value,
    mutation_oracle: &Value,
) -> Result<(), String> {
    if expected.is_empty() {
        return Ok(());
    }
    let divergence_observed = divergence
        .get("documentedDifferences")
        .and_then(Value::as_array)
        .ok_or_else(|| divergence_error("comparison is missing documented differences"))?;
    let empty_mutation_observed = Vec::new();
    let mutation_observed = mutation_oracle
        .get("documentedDifferences")
        .and_then(Value::as_array)
        .unwrap_or(&empty_mutation_observed)
        .iter();
    let observed_keys = divergence_observed
        .iter()
        .chain(mutation_observed)
        .map(documented_key)
        .collect::<BTreeSet<_>>();
    let stale = expected
        .iter()
        .filter(|difference| {
            let value = documented_json(difference);
            !observed_keys.contains(&documented_key(&value))
        })
        .collect::<Vec<_>>();
    if stale.is_empty() {
        return Ok(());
    }
    let locations = stale
        .iter()
        .map(|difference| {
            format!(
                "{}:{}:{}",
                difference.file, difference.line, difference.column
            )
        })
        .collect::<Vec<_>>()
        .join("\n- ");
    Err(format!(
        "Documented typecheck difference ledger is stale for {project_id}: {} of {} entries did not reproduce; remove converged rows from tests/_fixtures/compat-documented-differences.json:\n- {locations}",
        stale.len(),
        expected.len(),
    ))
}

fn pair_documented_differences(
    expected: &[DocumentedDifference],
    false_positives: &mut Vec<Diagnostic>,
    false_negatives: &mut Vec<Diagnostic>,
    message_mismatches: &mut Vec<SharedDiagnostic>,
) -> Vec<DocumentedDifference> {
    let mut paired = Vec::new();
    for difference in expected {
        match (difference.vize.as_ref(), difference.baseline.as_ref()) {
            (Some(vize), Some(baseline)) => {
                let positive_index = find_documented(false_positives, difference, vize);
                let negative_index = find_documented(false_negatives, difference, baseline);
                if let (Some(positive), Some(negative)) = (positive_index, negative_index) {
                    false_positives.remove(positive);
                    false_negatives.remove(negative);
                    paired.push(difference.clone());
                    continue;
                }
                if let Some(index) = find_documented_mismatch(message_mismatches, difference) {
                    message_mismatches.remove(index);
                    paired.push(difference.clone());
                }
            }
            (Some(vize), None) => {
                if let Some(positive) = find_documented(false_positives, difference, vize) {
                    false_positives.remove(positive);
                    paired.push(difference.clone());
                }
            }
            (None, Some(baseline)) => {
                if let Some(negative) = find_documented(false_negatives, difference, baseline) {
                    false_negatives.remove(negative);
                    paired.push(difference.clone());
                }
            }
            (None, None) => {}
        }
    }
    paired
}

fn find_documented(
    records: &[Diagnostic],
    difference: &DocumentedDifference,
    side: &DiagnosticSide,
) -> Option<usize> {
    records.iter().position(|candidate| {
        candidate.file == difference.file
            && candidate.severity == difference.severity
            && candidate.line == difference.line
            && candidate.column == difference.column
            && candidate.code == side.code
            && candidate.message == side.message
    })
}

fn find_documented_mismatch(
    records: &[SharedDiagnostic],
    difference: &DocumentedDifference,
) -> Option<usize> {
    let vize = difference.vize.as_ref()?;
    let baseline = difference.baseline.as_ref()?;
    records.iter().position(|candidate| {
        candidate.file == difference.file
            && candidate.severity == difference.severity
            && candidate.line == difference.line
            && candidate.column == difference.column
            && candidate.code == vize.code
            && candidate.code == baseline.code
            && candidate.vize_message == vize.message
            && candidate.baseline_message == baseline.message
    })
}

fn read_documented_differences(path: &Path) -> Result<Vec<DocumentedDifference>, String> {
    let ledger = common::read_json(path)?;
    if ledger.get("schema").and_then(Value::as_str) != Some("vize.compatDocumentedDifferences")
        || ledger.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err("Documented difference ledger schema is unsupported".to_string());
    }
    let entries = ledger
        .get("differences")
        .and_then(Value::as_array)
        .ok_or_else(|| "Documented difference ledger must list differences".to_string())?;
    entries
        .iter()
        .enumerate()
        .map(|(index, value)| parse_documented_difference(value, index))
        .collect()
}

fn parse_documented_difference(
    value: &Value,
    index: usize,
) -> Result<DocumentedDifference, String> {
    let label = format!("documented difference {index}");
    let project = value
        .get("project")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| divergence_error(&format!("{label} must name a project")))?
        .to_string();
    let file = value
        .get("file")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.ends_with(".vue"))
        .ok_or_else(|| divergence_error(&format!("{label} must reference a .vue file")))?
        .to_string();
    let severity = value
        .get("severity")
        .and_then(Value::as_str)
        .filter(|value| *value == "error" || *value == "warning")
        .ok_or_else(|| divergence_error(&format!("{label}.severity must be error or warning")))?
        .to_string();
    let line = positive_json_integer(value.get("line"), &format!("{label}.line"))?;
    let column = positive_json_integer(value.get("column"), &format!("{label}.column"))?;
    let vize = optional_documented_side(value.get("vize"), &format!("{label}.vize"))?;
    let baseline = optional_documented_side(value.get("baseline"), &format!("{label}.baseline"))?;
    if vize.is_none() && baseline.is_none() {
        invalid_divergence(&format!("{label} must record at least one tool side"))?;
    }
    if let (Some(vize), Some(baseline)) = (&vize, &baseline)
        && vize.code == baseline.code
        && vize.message == baseline.message
    {
        invalid_divergence(&format!(
            "{label} must record a difference between the two tools"
        ))?;
    }
    let issue = positive_json_integer(value.get("issue"), &format!("{label}.issue"))?;
    let reason = normalize_message(value.get("reason").and_then(Value::as_str).unwrap_or(""));
    if reason.len() < 40 {
        invalid_divergence(&format!(
            "{label}.reason must explain why the difference is expected"
        ))?;
    }
    Ok(DocumentedDifference {
        project,
        file,
        severity,
        line,
        column,
        vize,
        baseline,
        issue,
        reason,
    })
}

fn optional_documented_side(
    value: Option<&Value>,
    label: &str,
) -> Result<Option<DiagnosticSide>, String> {
    match value {
        Some(value) if value.is_null() => Ok(None),
        Some(_) => documented_side(value, label).map(Some),
        None => Ok(None),
    }
}

fn documented_side(value: Option<&Value>, label: &str) -> Result<DiagnosticSide, String> {
    let value = value.ok_or_else(|| divergence_error(&format!("{label} must be an object")))?;
    let code = positive_json_integer(value.get("code"), &format!("{label}.code"))?;
    let message = normalize_message(value.get("message").and_then(Value::as_str).unwrap_or(""));
    if message.is_empty() {
        invalid_divergence(&format!("{label}.message must be non-empty"))?;
    }
    Ok(DiagnosticSide { code, message })
}

fn positive_json_integer(value: Option<&Value>, label: &str) -> Result<u64, String> {
    value
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| divergence_error(&format!("{label} must be a positive safe integer")))
}

fn select_documented_differences(
    values: &[DocumentedDifference],
    project_id: &str,
    cwd: &Path,
) -> Result<Vec<DocumentedDifference>, String> {
    let mut selected = Vec::new();
    let mut identities = BTreeSet::new();
    for value in values {
        let file = normalize_vize_path(Some(&value.file), cwd, "documented difference file")?;
        let identity = format!(
            "{}\0{}\0{}\0{}\0{}",
            value.project, file, value.severity, value.line, value.column
        );
        if !identities.insert(identity) {
            invalid_divergence(
                "documented difference duplicates an earlier documented difference",
            )?;
        }
        if value.project != project_id {
            continue;
        }
        selected.push(DocumentedDifference {
            file,
            ..value.clone()
        });
    }
    selected.sort_by(compare_documented);
    Ok(selected)
}

fn comparable_vue_file_set(report: &Value, cwd: &Path) -> Result<BTreeSet<String>, String> {
    let files = report
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| divergence_error("Vize report must contain files"))?;
    let count = report
        .get("fileCount")
        .and_then(Value::as_u64)
        .unwrap_or(files.len() as u64) as usize;
    files
        .iter()
        .take(count)
        .enumerate()
        .filter_map(|(index, entry)| {
            let file = normalize_vize_path(
                entry.get("file").and_then(Value::as_str),
                cwd,
                &format!("Vize files[{index}].file"),
            );
            match file {
                Ok(file) if file.ends_with(".vue") => Some(Ok(file)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect()
}

fn record(
    file: String,
    severity: String,
    line: &str,
    column: &str,
    code: &str,
    message: &str,
) -> Result<Diagnostic, String> {
    let line = parse_positive_number(line, &file)?;
    let column = parse_positive_number(column, &file)?;
    let code = parse_positive_number(code, &file)?;
    let message = normalize_message(message);
    if message.is_empty() {
        invalid_divergence(&format!("diagnostic message must be non-empty: {file}"))?;
    }
    Ok(Diagnostic {
        file,
        severity,
        line,
        column,
        code,
        message,
    })
}

fn parse_positive_number(value: &str, file: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            divergence_error(&format!(
                "diagnostic range and code must be positive safe integers: {file}"
            ))
        })
}

fn normalize_vize_path(value: Option<&str>, cwd: &Path, label: &str) -> Result<String, String> {
    let value = value
        .filter(|value| !value.is_empty())
        .ok_or_else(|| divergence_error(&format!("{label} must be non-empty")))?;
    let mut normalized = value.replace('\\', "/");
    if is_absolute_program_path(&normalized) {
        normalized = pathdiff::diff_paths(Path::new(&normalized), cwd)
            .map(|path| common::normalize_path(&path))
            .unwrap_or(normalized);
    }
    if let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    let segments = normalized.split('/').collect::<Vec<_>>();
    if normalized.is_empty()
        || is_absolute_program_path(&normalized)
        || segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        invalid_divergence(&format!("{label} must stay inside the fixture workspace"))?;
    }
    Ok(normalized)
}

fn normalize_baseline_path(value: &str, cwd: &Path) -> Result<Option<String>, String> {
    if value.is_empty() {
        invalid_divergence("vue-tsc diagnostic file must be non-empty")?;
    }
    let mut normalized = value.replace('\\', "/");
    if is_absolute_program_path(&normalized) {
        normalized = pathdiff::diff_paths(Path::new(&normalized), cwd)
            .map(|path| common::normalize_path(&path))
            .unwrap_or(normalized);
    }
    if let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    let segments = normalized.split('/').collect::<Vec<_>>();
    if normalized.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == ".")
    {
        invalid_divergence("vue-tsc diagnostic file must be normalized")?;
    }
    if is_absolute_program_path(&normalized) || segments.contains(&"..") {
        return Ok(None);
    }
    Ok(Some(normalized))
}

fn normalize_message(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn invalid_divergence<T>(message: &str) -> Result<T, String> {
    Err(divergence_error(message))
}

fn divergence_error(message: &str) -> String {
    format!("Invalid typecheck divergence input: {message}")
}

struct SupportVueClassifier {
    source_roots: Vec<String>,
}

impl SupportVueClassifier {
    fn new(cwd: &Path, source_roots: &[PathBuf]) -> Self {
        let mut source_roots = source_roots
            .iter()
            .filter_map(|source_root| pathdiff::diff_paths(source_root, cwd))
            .map(|relative| {
                let normalized = common::normalize_path(&relative);
                if normalized.is_empty() {
                    ".".to_string()
                } else {
                    normalized
                }
            })
            .collect::<Vec<_>>();
        sort_bytes_dedup(&mut source_roots);
        Self { source_roots }
    }

    fn is_support_vue_file(&self, file: &str) -> bool {
        has_dot_directory_parent(file) || self.is_outside_source_roots(file)
    }

    fn is_outside_source_roots(&self, file: &str) -> bool {
        !self.source_roots.is_empty()
            && !self
                .source_roots
                .iter()
                .any(|root| relative_file_is_inside(root, file))
    }
}

fn has_dot_directory_parent(file: &str) -> bool {
    file.split('/')
        .collect::<Vec<_>>()
        .split_last()
        .map(|(_, parents)| parents.iter().any(|segment| segment.starts_with('.')))
        .unwrap_or(false)
}

fn relative_file_is_inside(root: &str, file: &str) -> bool {
    root == "."
        || file == root
        || file
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn is_absolute_program_path(file: &str) -> bool {
    file.starts_with('/') || Regex::new(r"^[A-Za-z]:/").unwrap().is_match(file)
}

fn render_configuration_diagnostic(diagnostic: &Value) -> String {
    let severity = diagnostic
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or("error");
    let code = diagnostic.get("code").and_then(Value::as_u64).unwrap_or(0);
    let message = diagnostic
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("");
    match (
        diagnostic.get("file").and_then(Value::as_str),
        diagnostic.get("line").and_then(Value::as_u64),
        diagnostic.get("column").and_then(Value::as_u64),
    ) {
        (Some(file), Some(line), Some(column)) => {
            format!("{file}({line},{column}): {severity} TS{code}: {message}")
        }
        _ => format!("{severity} TS{code}: {message}"),
    }
}

fn parse_package_root(file: &str) -> Option<(String, String)> {
    let marker = "/node_modules/";
    let index = file.rfind(marker)?;
    let base = index + marker.len();
    let segments = file[base..].split('/').collect::<Vec<_>>();
    if segments
        .first()
        .is_none_or(|segment| *segment == ".pnpm" || segment.is_empty())
    {
        return None;
    }
    let name = if segments[0].starts_with('@') {
        if segments.len() > 1 && !segments[1].is_empty() {
            format!("{}/{}", segments[0], segments[1])
        } else {
            return None;
        }
    } else {
        segments[0].to_string()
    };
    Some((name.clone(), format!("{}{}", &file[..base], name)))
}

fn describe_package(name: &str, copies: &HashMap<String, bool>, fixture_root: &Path) -> Value {
    let mut paths = copies.keys().cloned().collect::<Vec<_>>();
    paths.sort();
    let copy_values = paths
        .iter()
        .map(|root| {
            json!({
                "path": pathdiff::diff_paths(Path::new(root), fixture_root)
                    .map(|path| common::normalize_path(&path))
                    .unwrap_or_else(|| root.clone()),
                "insideFixture": copies.get(root).copied().unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "name": name,
        "copies": copy_values,
        "insideFixtureCount": paths.iter().filter(|root| copies.get(*root).copied().unwrap_or(false)).count(),
        "outsideFixtureCount": paths.iter().filter(|root| !copies.get(*root).copied().unwrap_or(false)).count(),
    })
}

fn first_ambient_failure(vue_runtime: &[Value]) -> Option<String> {
    for entry in vue_runtime {
        let copies = entry
            .get("copies")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if copies.len() > 1 {
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("vue");
            let paths = copies
                .iter()
                .filter_map(|copy| copy.get("path").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(", ");
            return Some(format!(
                "vue-tsc resolved {} copies of '{name}' into the baseline program ({paths}), so the fixture's own module augmentations merged into a different module identity than its components",
                copies.len()
            ));
        }
    }
    for entry in vue_runtime {
        if entry
            .get("insideFixtureCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            == 0
        {
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("vue");
            let path = entry
                .pointer("/copies/0/path")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            return Some(format!(
                "vue-tsc resolved '{name}' outside the fixture ({path}), so the baseline measured a type environment the fixture does not own"
            ));
        }
    }
    None
}

fn matches_seeded_probe(record: &Value, diagnostic: &Diagnostic) -> bool {
    record.get("file").and_then(Value::as_str) == Some(diagnostic.file.as_str())
        && record.get("severity").and_then(Value::as_str) == Some(diagnostic.severity.as_str())
        && record.get("code").and_then(Value::as_u64) == Some(diagnostic.code)
}

fn comparison_has_diagnostic(comparison: &Value, diagnostic: &Diagnostic) -> bool {
    let mut records = Vec::new();
    for key in ["shared", "messageMismatches"] {
        for record in comparison
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut vize = record.clone();
            vize["message"] = record.get("vizeMessage").cloned().unwrap_or(Value::Null);
            let mut baseline = record.clone();
            baseline["message"] = record
                .get("baselineMessage")
                .cloned()
                .unwrap_or(Value::Null);
            records.push(vize);
            records.push(baseline);
        }
    }
    for key in ["falsePositives", "falseNegatives"] {
        records.extend(
            comparison
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned(),
        );
    }
    records.iter().any(|record| {
        record.get("file").and_then(Value::as_str) == Some(diagnostic.file.as_str())
            && record.get("severity").and_then(Value::as_str) == Some(diagnostic.severity.as_str())
            && record.get("line").and_then(Value::as_u64) == Some(diagnostic.line)
            && record.get("column").and_then(Value::as_u64) == Some(diagnostic.column)
            && record.get("code").and_then(Value::as_u64) == Some(diagnostic.code)
            && record.get("message").and_then(Value::as_str) == Some(diagnostic.message.as_str())
    })
}

fn summary_evidence(summary: &Value) -> Value {
    json!({
        "vizeDiagnosticCount": summary.get("vizeDiagnosticCount").and_then(Value::as_u64).unwrap_or(0),
        "baselineDiagnosticCount": summary.get("baselineDiagnosticCount").and_then(Value::as_u64).unwrap_or(0),
        "sharedCount": summary.get("sharedCount").and_then(Value::as_u64).unwrap_or(0),
        "messageMismatchCount": summary.get("messageMismatchCount").and_then(Value::as_u64).unwrap_or(0),
        "documentedDifferenceCount": summary.get("documentedDifferenceCount").and_then(Value::as_u64).unwrap_or(0),
        "falsePositiveCount": summary.get("falsePositiveCount").and_then(Value::as_u64).unwrap_or(0),
        "falseNegativeCount": summary.get("falseNegativeCount").and_then(Value::as_u64).unwrap_or(0),
    })
}

fn run_evidence(run: &ToolRun) -> Value {
    json!({
        "command": run.command,
        "exitCode": run.status,
        "stdoutSha256": sha256(&run.stdout),
        "stderrSha256": sha256(&run.stderr),
    })
}

fn shared_key(record: &Value) -> String {
    [
        value_string(record, "file"),
        value_string(record, "severity"),
        value_number(record, "line").to_string(),
        value_number(record, "column").to_string(),
        value_number(record, "code").to_string(),
        value_string(record, "vizeMessage"),
        value_string(record, "baselineMessage"),
    ]
    .join("\0")
}

fn documented_key(record: &Value) -> String {
    [
        value_string(record, "project"),
        value_string(record, "file"),
        value_string(record, "severity"),
        value_number(record, "line").to_string(),
        value_number(record, "column").to_string(),
        record
            .pointer("/vize/code")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .to_string(),
        record
            .pointer("/vize/message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        record
            .pointer("/baseline/code")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .to_string(),
        record
            .pointer("/baseline/message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    ]
    .join("\0")
}

fn diagnostic_key(record: &Value) -> String {
    [
        value_string(record, "file"),
        value_string(record, "severity"),
        value_number(record, "line").to_string(),
        value_number(record, "column").to_string(),
        value_number(record, "code").to_string(),
        value_string(record, "message"),
    ]
    .join("\0")
}

fn value_string(record: &Value, key: &str) -> String {
    record
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn value_number(record: &Value, key: &str) -> u64 {
    record.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn build_seeded_mutation(clean_source: &str) -> Option<(String, u64, u64)> {
    let blocks = find_script_blocks(clean_source);
    if let Some(block) = blocks
        .iter()
        .find(|block| block.setup && block.typescript && block.close_index.is_some())
    {
        let close_index = block.close_index?;
        let before_close = &clean_source[..close_index];
        let separator = if before_close.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let prefix = format!("{before_close}{separator}");
        return Some((
            format!(
                "{prefix}const __vize_typecheck_mutation_probe: string = 1\nvoid __vize_typecheck_mutation_probe\n{}",
                &clean_source[close_index..]
            ),
            line_count(&prefix),
            1,
        ));
    }
    let has_setup = blocks.iter().any(|block| block.setup);
    if !has_setup {
        return Some(appended_mutation(clean_source, "script setup"));
    }
    let has_normal_script = blocks.iter().any(|block| !block.setup);
    if has_normal_script {
        None
    } else {
        Some(appended_mutation(clean_source, "script"))
    }
}

struct ScriptBlock {
    setup: bool,
    typescript: bool,
    close_index: Option<usize>,
}

fn find_script_blocks(source: &str) -> Vec<ScriptBlock> {
    let comment = Regex::new(r"(?s)<!--.*?-->").unwrap();
    let scannable = comment
        .replace_all(source, |captures: &regex::Captures<'_>| {
            " ".repeat(captures[0].len())
        })
        .to_string();
    let pattern = Regex::new(r"(?i)<script\b([^>]*)>").unwrap();
    pattern
        .captures_iter(&scannable)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            let attrs = captures.get(1).map(|m| m.as_str()).unwrap_or("");
            let setup = Regex::new(r#"(?i)\bsetup(?:\s|=|>|$)"#)
                .unwrap()
                .is_match(attrs);
            let typescript = Regex::new(r#"(?i)\blang\s*=\s*(?:"tsx?"|'tsx?'|tsx?)(?:\s|>|$)"#)
                .unwrap()
                .is_match(attrs);
            Some(ScriptBlock {
                setup,
                typescript,
                close_index: source[whole.end()..]
                    .find("</script>")
                    .map(|index| whole.end() + index),
            })
        })
        .collect()
}

fn appended_mutation(clean_source: &str, tag: &str) -> (String, u64, u64) {
    let prefix = if clean_source.ends_with('\n') {
        clean_source.to_string()
    } else {
        format!("{clean_source}\n")
    };
    let before_probe = format!("{prefix}<{tag} lang=\"ts\">\n");
    (
        format!(
            "{before_probe}const __vize_typecheck_mutation_probe: string = 1\nvoid __vize_typecheck_mutation_probe\n</script>\n"
        ),
        line_count(&before_probe),
        1,
    )
}

fn line_count(value: &str) -> u64 {
    value.replace("\r\n", "\n").split('\n').count() as u64
}

fn unusable_mutation(seed: &str, reason: &str) -> Value {
    json!({
        "schema": "vize.fixtureTypecheckSeededMutationOracle",
        "version": 1,
        "seed": seed,
        "verdict": "unusable",
        "passed": false,
        "unusableReason": reason,
        "file": Value::Null,
        "sourceSha256": Value::Null,
        "span": Value::Null,
        "diagnostic": Value::Null,
        "states": [],
    })
}

fn optional_typecheck_corpus_globs(project: &Value) -> Option<Vec<String>> {
    let globs = project
        .pointer("/typecheckPerformance/corpusGlobs")
        .and_then(Value::as_array)
        .or_else(|| project.get("vueGlobs").and_then(Value::as_array))?;
    Some(
        globs
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
    )
}

fn expected_typecheck_vue_files(
    cwd: &Path,
    project: &Value,
    vize_report: &Value,
) -> Result<Vec<String>, String> {
    if let Some(globs) = optional_typecheck_corpus_globs(project) {
        return collect_vue_input_paths(cwd, &globs);
    }
    let mut files = comparable_vue_file_set(vize_report, cwd)?
        .into_iter()
        .collect::<Vec<_>>();
    sort_bytes_dedup(&mut files);
    Ok(files)
}

fn typecheck_tsconfig_path(project: &Value) -> Option<String> {
    project
        .pointer("/typecheckPerformance/baseline/tsconfig")
        .and_then(Value::as_str)
        .or_else(|| project.get("tsconfig").and_then(Value::as_str))
        .map(str::to_string)
}

fn assert_source_unchanged(
    state: &str,
    file: &str,
    source_path: &Path,
    expected_sha256: &str,
    tool: &str,
) -> Result<(), String> {
    let actual = sha256_bytes(
        &fs::read(source_path)
            .map_err(|error| format!("cannot read {}: {error}", source_path.display()))?,
    );
    if actual != expected_sha256 {
        return Err(format!(
            "{tool} mutated {file} during seeded {state} oracle run"
        ));
    }
    Ok(())
}

fn run_baseline_command(
    command: &Path,
    args: &[String],
    cwd: &Path,
    timeout_ms: u64,
    label: &str,
    allowed_statuses: &[i32],
) -> Result<CapturedToolRun, String> {
    match run_capture_limited(command, args, cwd, timeout_ms, label, allowed_statuses) {
        Ok(run) => Ok(CapturedToolRun {
            run,
            run_error: None,
        }),
        Err(error) if error == timeout_error(label, timeout_ms) => Ok(CapturedToolRun {
            run: failed_tool_run(command, args, timeout_ms as u128, &error),
            run_error: Some(error),
        }),
        Err(error) => Err(error),
    }
}

fn skipped_baseline_command(
    command: &Path,
    args: &[String],
    label: &str,
    reason: &str,
) -> CapturedToolRun {
    let _ = label;
    CapturedToolRun {
        run: failed_tool_run(command, args, 0, reason),
        run_error: Some(reason.to_string()),
    }
}

fn failed_tool_run(command: &Path, args: &[String], duration_ms: u128, reason: &str) -> ToolRun {
    ToolRun {
        command: display_command(command, args),
        status: -1,
        stdout: String::new(),
        stderr: reason.to_string(),
        output: format!("\n{reason}"),
        duration_ms,
        parsed: None,
    }
}

fn captured_exit_code(captured: &CapturedToolRun) -> Value {
    if captured.run_error.is_some() {
        Value::Null
    } else {
        json!(captured.run.status)
    }
}

fn run_capture_limited(
    command: &Path,
    args: &[String],
    cwd: &Path,
    timeout_ms: u64,
    label: &str,
    allowed_statuses: &[i32],
) -> Result<ToolRun, String> {
    let started = Instant::now();
    let mut child_command = Command::new(command);
    child_command
        .args(args)
        .current_dir(cwd)
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    child_command.process_group(0);
    let mut child = child_command
        .spawn()
        .map_err(|error| format!("{label} failed to run: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label} failed to capture stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{label} failed to capture stderr"))?;
    let stdout_reader = thread::spawn(move || read_child_output(stdout));
    let stderr_reader = thread::spawn(move || read_child_output(stderr));
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("{label} failed to run: {error}"))?
        {
            break status;
        }
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            kill_child_group(&mut child);
            // Timeout artifacts only need the failure reason. Descendants may
            // keep inherited stdio open, so this path must not join readers.
            drop(stdout_reader);
            drop(stderr_reader);
            return Err(timeout_error(label, timeout_ms));
        }
        thread::sleep(Duration::from_millis(5));
    };
    let status = status.code().unwrap_or(1);
    kill_child_process_group(child.id());
    let stdout = join_output_reader(stdout_reader, label, "stdout")?;
    let stderr = join_output_reader(stderr_reader, label, "stderr")?;
    if !allowed_statuses.contains(&status) {
        return Err(format!("{label} exited with unsupported status {status}"));
    }
    let stdout = String::from_utf8_lossy(&stdout).into_owned();
    let stderr = String::from_utf8_lossy(&stderr).into_owned();
    Ok(ToolRun {
        command: display_command(command, args),
        status,
        output: format!("{stdout}\n{stderr}"),
        stdout,
        stderr,
        duration_ms: started.elapsed().as_millis(),
        parsed: None,
    })
}

fn read_child_output<R: Read>(mut pipe: R) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    pipe.read_to_end(&mut output)
        .map_err(|error| format!("failed to read child output: {error}"))?;
    Ok(output)
}

fn join_output_reader(
    reader: thread::JoinHandle<Result<Vec<u8>, String>>,
    label: &str,
    stream: &str,
) -> Result<Vec<u8>, String> {
    reader
        .join()
        .map_err(|_| format!("{label} {stream} reader panicked"))?
        .map_err(|error| format!("{label} failed to run: {error}"))
}

fn timeout_error(label: &str, timeout_ms: u64) -> String {
    format!("{label} failed to run: spawn timed out after {timeout_ms}ms")
}

fn kill_child_group(child: &mut std::process::Child) {
    kill_child_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn kill_child_process_group(child_id: u32) {
    let Ok(child_pid) = i32::try_from(child_id) else {
        return;
    };
    // SAFETY: `child_pid` came from `Child::id`, and a negative pid asks the OS
    // to signal the process group created by `process_group(0)` above.
    unsafe {
        let _ = kill(-child_pid, SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_child_process_group(_child_id: u32) {}

#[cfg(unix)]
const SIGKILL: i32 = 9;

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

fn mutation_run_error(error: String) -> String {
    error.replace(" mutation run failed to run:", " mutation run failed:")
}

fn normalize_display_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn display_command(command: &Path, args: &[String]) -> String {
    std::iter::once(command.display().to_string())
        .chain(args.iter().cloned())
        .map(|part| common::shell_quote(&part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_relative(root: &Path, path: &Path) -> String {
    pathdiff::diff_paths(path, root)
        .map(|path| common::normalize_path(&path))
        .unwrap_or_else(|| path.display().to_string())
}

fn ratio(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

fn validate_typechecker_output(
    project: &Value,
    output: &Value,
    exit_code: i32,
    expected_files: Option<&[String]>,
    authored_files: Option<&[String]>,
) -> Result<Value, String> {
    let expected_output_keys = if output.get("programs").is_some() {
        vec![
            "errorCount",
            "fileCount",
            "files",
            "programs",
            "warningCount",
        ]
    } else {
        vec!["errorCount", "fileCount", "files", "warningCount"]
    };
    require_exact_keys(
        output,
        &expected_output_keys,
        "invalid typechecker JSON output: envelope keys are invalid",
    )?;
    for field in ["errorCount", "warningCount", "fileCount"] {
        if output.get(field).and_then(Value::as_u64).is_none() {
            invalid_typechecker(&format!("{field} must be a non-negative safe integer"))?;
        }
    }
    let files = output
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| typechecker_error("files must be an array"))?;
    let file_count = output.get("fileCount").and_then(Value::as_u64).unwrap_or(0) as usize;
    if file_count > files.len() {
        invalid_typechecker(&format!(
            "fileCount {file_count} exceeds {} file entries",
            files.len()
        ))?;
    }
    if project.get("expectedVueFileCount").and_then(Value::as_u64) == Some(0) && file_count != 0 {
        invalid_typechecker(&format!(
            "expected zero checked files, received {file_count}"
        ))?;
    }
    if project.get("expectedVueFileCount").and_then(Value::as_u64) != Some(0) && file_count == 0 {
        invalid_typechecker("non-empty fixture checked zero Vue files")?;
    }

    let mut seen = BTreeSet::new();
    let mut error_count = 0u64;
    let mut warning_count = 0u64;
    for (index, file) in files.iter().enumerate() {
        require_exact_keys(
            file,
            &["diagnostics", "file"],
            &format!("invalid typechecker JSON output: files[{index}] keys are invalid"),
        )?;
        let file_name = file.get("file").and_then(Value::as_str).ok_or_else(|| {
            typechecker_error(&format!(
                "files[{index}].file must be a normalized relative path"
            ))
        })?;
        require_relative_path(file_name, &format!("files[{index}].file"))?;
        if !seen.insert(file_name.to_string()) {
            invalid_typechecker(&format!("duplicate file entry: {file_name}"))?;
        }
        let diagnostics = file
            .get("diagnostics")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                typechecker_error(&format!("files[{index}].diagnostics must be an array"))
            })?;
        if index < file_count && expected_files.is_none() && !file_name.ends_with(".vue") {
            invalid_typechecker(&format!("checked file is not a Vue SFC: {file_name}"))?;
        }
        if index < file_count && !is_typecheck_source(file_name) {
            invalid_typechecker(&format!(
                "checked file has an unsupported typecheck extension: {file_name}"
            ))?;
        }
        if index >= file_count && diagnostics.is_empty() {
            invalid_typechecker(&format!(
                "project-level file entry has no diagnostics: {file_name}"
            ))?;
        }
        for (diagnostic_index, diagnostic) in diagnostics.iter().enumerate() {
            let diagnostic = diagnostic.as_str().ok_or_else(|| {
                typechecker_error(&format!(
                    "files[{index}].diagnostics[{diagnostic_index}] must be a non-empty string"
                ))
            })?;
            if diagnostic.is_empty() {
                invalid_typechecker(&format!(
                    "files[{index}].diagnostics[{diagnostic_index}] must be a non-empty string"
                ))?;
            }
            if diagnostic.starts_with("error:") {
                error_count += 1;
            } else if diagnostic.starts_with("warning:") {
                warning_count += 1;
            } else {
                invalid_typechecker(&format!(
                    "diagnostic has no error or warning prefix: {file_name}"
                ))?;
            }
        }
    }
    let checked_files = files
        .iter()
        .take(file_count)
        .filter_map(|file| file.get("file").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut sorted_checked = checked_files.clone();
    sorted_checked.sort_by(|left, right| compare_bytes(left, right));
    if checked_files != sorted_checked {
        invalid_typechecker("checked file entries are not sorted")?;
    }

    let mut requested_files = checked_files.clone();
    let mut transitive_authored_files = Vec::new();
    let mut transitive_dependency_files = Vec::new();
    if let Some(expected_files) = expected_files {
        validate_manifest_input(expected_files, "requested fixture inputs", is_vue_sfc)?;
        let authored_files = authored_files.unwrap_or(expected_files);
        validate_manifest_input(
            authored_files,
            "authored fixture sources",
            is_typecheck_source,
        )?;
        let checked_set = checked_files.iter().cloned().collect::<BTreeSet<_>>();
        let expected_set = expected_files.iter().cloned().collect::<BTreeSet<_>>();
        let missing = expected_files
            .iter()
            .filter(|file| !checked_set.contains(*file))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            invalid_typechecker(&format!(
                "checked files are missing requested fixture inputs: [{}]",
                missing.join(", ")
            ))?;
        }
        let authored_set = authored_files.iter().cloned().collect::<BTreeSet<_>>();
        let missing_authored_inputs = expected_files
            .iter()
            .filter(|file| !authored_set.contains(*file))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_authored_inputs.is_empty() {
            invalid_typechecker(&format!(
                "requested fixture inputs are not authored sources: [{}]",
                missing_authored_inputs.join(", ")
            ))?;
        }
        let transitive_files = checked_files
            .iter()
            .filter(|file| !expected_set.contains(*file))
            .cloned()
            .collect::<Vec<_>>();
        transitive_authored_files = transitive_files
            .iter()
            .filter(|file| !is_dependency_source(file))
            .cloned()
            .collect::<Vec<_>>();
        transitive_dependency_files = transitive_files
            .iter()
            .filter(|file| is_dependency_source(file))
            .cloned()
            .collect::<Vec<_>>();
        let unclassified = transitive_authored_files
            .iter()
            .filter(|file| !authored_set.contains(*file))
            .cloned()
            .collect::<Vec<_>>();
        if !unclassified.is_empty() {
            invalid_typechecker(&format!(
                "checked transitive files are not authored fixture sources: [{}]",
                unclassified.join(", ")
            ))?;
        }
        requested_files = expected_files.to_vec();
    }
    if output.get("errorCount").and_then(Value::as_u64) != Some(error_count) {
        invalid_typechecker(&format!(
            "errorCount {} does not match {error_count} diagnostics",
            output
                .get("errorCount")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ))?;
    }
    if output.get("warningCount").and_then(Value::as_u64) != Some(warning_count) {
        invalid_typechecker(&format!(
            "warningCount {} does not match {warning_count} diagnostics",
            output
                .get("warningCount")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ))?;
    }
    let expected_exit_code = if error_count > 0 { 1 } else { 0 };
    if exit_code != expected_exit_code {
        invalid_typechecker(&format!(
            "exit code {exit_code} does not match expected {expected_exit_code}"
        ))?;
    }
    Ok(json!({
        "schema": "vize.fixtureTypecheckerCoverage",
        "version": 2,
        "requested": create_manifest(&requested_files),
        "transitiveAuthored": create_manifest(&transitive_authored_files),
        "transitiveDependencies": create_manifest(&transitive_dependency_files),
        "checked": create_manifest(&checked_files),
    }))
}

fn summarize_typechecker_coverage(coverage: &Value) -> Result<Value, String> {
    require_exact_keys(
        coverage,
        &[
            "checked",
            "requested",
            "schema",
            "transitiveAuthored",
            "transitiveDependencies",
            "version",
        ],
        "invalid typechecker JSON output: typechecker coverage keys are invalid",
    )?;
    if coverage.get("schema").and_then(Value::as_str) != Some("vize.fixtureTypecheckerCoverage")
        || coverage.get("version").and_then(Value::as_u64) != Some(2)
    {
        invalid_typechecker("typechecker coverage schema is unsupported")?;
    }
    Ok(json!({
        "requestedFileCount": coverage.pointer("/requested/fileCount").and_then(Value::as_u64).unwrap_or(0),
        "requestedSha256": coverage.pointer("/requested/sha256").and_then(Value::as_str).unwrap_or(""),
        "transitiveAuthoredFileCount": coverage.pointer("/transitiveAuthored/fileCount").and_then(Value::as_u64).unwrap_or(0),
        "transitiveAuthoredSha256": coverage.pointer("/transitiveAuthored/sha256").and_then(Value::as_str).unwrap_or(""),
        "transitiveDependencyFileCount": coverage.pointer("/transitiveDependencies/fileCount").and_then(Value::as_u64).unwrap_or(0),
        "transitiveDependencySha256": coverage.pointer("/transitiveDependencies/sha256").and_then(Value::as_str).unwrap_or(""),
        "checkedFileCount": coverage.pointer("/checked/fileCount").and_then(Value::as_u64).unwrap_or(0),
        "checkedSha256": coverage.pointer("/checked/sha256").and_then(Value::as_str).unwrap_or(""),
    }))
}

fn collect_vue_input_paths(cwd: &Path, patterns: &[String]) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for pattern in patterns {
        collect_pattern(cwd, pattern, &mut files)?;
    }
    sort_bytes_dedup(&mut files);
    Ok(files)
}

fn collect_typechecker_authored_paths(cwd: &Path) -> Result<Vec<String>, String> {
    let extensions = ["vue", "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];
    let mut files = Vec::new();
    collect_sources(cwd, cwd, &extensions, &mut files)?;
    sort_bytes_dedup(&mut files);
    Ok(files)
}

fn collect_pattern(cwd: &Path, pattern: &str, files: &mut Vec<String>) -> Result<(), String> {
    if let Some((root, extension)) = pattern.split_once("/**/*.") {
        collect_sources(cwd, &cwd.join(root), &[extension], files)?;
        return Ok(());
    }
    if let Some(extension) = pattern.strip_prefix("**/*.") {
        collect_sources(cwd, cwd, &[extension], files)?;
        return Ok(());
    }
    let candidate = cwd.join(pattern);
    if candidate.is_file() {
        files.push(common::normalize_path(
            candidate.strip_prefix(cwd).unwrap_or(&candidate),
        ));
    }
    Ok(())
}

fn collect_sources(
    cwd: &Path,
    dir: &Path,
    extensions: &[&str],
    files: &mut Vec<String>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).map_err(|error| format!("cannot read {}: {error}", dir.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name == "node_modules" || name == ".yarn" {
                continue;
            }
            collect_sources(cwd, &path, extensions, files)?;
        } else if path.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| extensions.contains(&extension))
        {
            files.push(common::normalize_path(
                path.strip_prefix(cwd).unwrap_or(&path),
            ));
        }
    }
    Ok(())
}

fn create_manifest(files: &[String]) -> Value {
    let mut bytes = Vec::new();
    for file in files {
        bytes.extend_from_slice(file.as_bytes());
        bytes.push(0);
    }
    json!({
        "fileCount": files.len(),
        "files": files,
        "sha256": sha256_bytes(&bytes),
    })
}

fn validate_manifest_input(
    files: &[String],
    label: &str,
    accepts_file: fn(&str) -> bool,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for (index, file) in files.iter().enumerate() {
        require_relative_path(file, &format!("{label}[{index}]"))?;
        if !accepts_file(file) {
            invalid_typechecker(&format!(
                "{label}[{index}] has an unsupported source extension"
            ))?;
        }
        if !seen.insert(file.clone()) {
            invalid_typechecker(&format!("{label} contains duplicate file: {file}"))?;
        }
    }
    let mut sorted = files.to_vec();
    sorted.sort_by(|left, right| compare_bytes(left, right));
    if files != sorted {
        invalid_typechecker(&format!("{label} are not sorted"))?;
    }
    Ok(())
}

fn require_relative_path(value: &str, label: &str) -> Result<(), String> {
    let segments = value.split('/').collect::<Vec<_>>();
    if value.is_empty()
        || is_absolute_program_path(value)
        || value.contains('\\')
        || value.starts_with("./")
        || segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        invalid_typechecker(&format!("{label} must be a normalized relative path"))?;
    }
    Ok(())
}

fn require_exact_keys(value: &Value, expected: &[&str], message: &str) -> Result<(), String> {
    let object = value.as_object().ok_or_else(|| message.to_string())?;
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    actual.sort();
    let mut expected = expected.to_vec();
    expected.sort();
    if actual != expected {
        return Err(message.to_string());
    }
    Ok(())
}

fn is_vue_sfc(file: &str) -> bool {
    file.ends_with(".vue")
}

fn is_typecheck_source(file: &str) -> bool {
    Regex::new(r"\.(?:vue|ts|tsx|mts|cts|js|jsx|mjs|cjs)$")
        .unwrap()
        .is_match(file)
}

fn is_dependency_source(file: &str) -> bool {
    file.split('/').any(|segment| segment == "node_modules")
}

fn invalid_typechecker<T>(message: &str) -> Result<T, String> {
    Err(typechecker_error(message))
}

fn typechecker_error(message: &str) -> String {
    format!("invalid typechecker JSON output: {message}")
}

fn evaluate_budget(
    performance: &Value,
    divergence: &Value,
    coverage: &Value,
    configuration: &Value,
    mutation_oracle: &Value,
    ambient: &Value,
) -> Result<Value, String> {
    let max_fp = ratio_field(performance, "maxFalsePositiveRatio")?;
    let max_fn = ratio_field(performance, "maxFalseNegativeRatio")?;
    let summary = &divergence["summary"];
    let fp_ratio = summary["falsePositiveRatio"].as_f64().unwrap_or(0.0);
    let fn_ratio = summary["falseNegativeRatio"].as_f64().unwrap_or(0.0);
    let false_positive_passed = fp_ratio <= max_fp;
    let false_negative_passed = fn_ratio <= max_fn;
    let unusable_reason = configuration
        .get("unusableReason")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| ambient_unusable_reason(ambient).map(str::to_string))
        .or_else(|| {
            coverage
                .get("unusableReason")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| mutation_oracle_unusable_reason(mutation_oracle).map(str::to_string))
        .or_else(|| diagnostic_mapping_unusable_reason(summary));
    let verdict = if unusable_reason.is_some() {
        "unusable"
    } else if false_positive_passed && false_negative_passed {
        "passed"
    } else {
        "breached"
    };
    Ok(json!({
        "maxFalsePositiveRatio": max_fp,
        "maxFalseNegativeRatio": max_fn,
        "falsePositivePassed": false_positive_passed,
        "falseNegativePassed": false_negative_passed,
        "unusableReason": unusable_reason,
        "verdict": verdict,
        "passed": verdict == "passed",
    }))
}

fn budget_failure_detail(artifact: &Value) -> Option<String> {
    let budget = &artifact["budget"];
    if budget.get("verdict").and_then(Value::as_str) == Some("passed") {
        return None;
    }
    let project = artifact["project"].as_str().unwrap_or("?");
    let unusable = budget.get("verdict").and_then(Value::as_str) == Some("unusable");
    let detail = if unusable {
        budget
            .get("unusableReason")
            .and_then(Value::as_str)
            .unwrap_or("budget failed")
            .to_string()
    } else {
        describe_breaches(artifact).join("; ")
    };
    Some(format!(
        "{} for {project} — {}: {detail}",
        if unusable {
            "Typecheck divergence baseline is unusable"
        } else {
            "Typecheck divergence budget breached"
        },
        describe_classification(artifact)
    ))
}

fn render_markdown(artifact: &Value) -> String {
    let summary = &artifact["divergence"]["summary"];
    let coverage = &artifact["baseline"]["coverage"];
    [
        format!(
            "## {} typecheck divergence",
            artifact["project"].as_str().unwrap_or("?")
        ),
        "".to_string(),
        format!(
            "Commit: {}",
            artifact
                .pointer("/evidence/commitSha")
                .and_then(Value::as_str)
                .unwrap_or("")
        ),
        format!(
            "Vize diagnostics: {}",
            number(summary, "vizeDiagnosticCount")
        ),
        format!(
            "vue-tsc diagnostics: {}",
            number(summary, "baselineDiagnosticCount")
        ),
        format!("Shared: {}", number(summary, "sharedCount")),
        format!(
            "Message mismatches: {}",
            number(summary, "messageMismatchCount")
        ),
        format!(
            "Documented differences: {}",
            number(summary, "documentedDifferenceCount")
        ),
        format!(
            "False positives: {} ({})",
            number(summary, "falsePositiveCount"),
            ratio_string(summary, "falsePositiveRatio")
        ),
        format!(
            "False negatives: {} ({})",
            number(summary, "falseNegativeCount"),
            ratio_string(summary, "falseNegativeRatio")
        ),
        format!(
            "Vize excluded non-Vue: {}",
            number(summary, "vizeExcludedNonVueCount")
        ),
        format!(
            "vue-tsc excluded non-Vue: {}",
            number(summary, "baselineExcludedNonVueCount")
        ),
        format!(
            "vue-tsc excluded support Vue: {}",
            number(summary, "baselineExcludedSupportVueCount")
        ),
        format!(
            "vue-tsc excluded project-level: {}",
            number(summary, "baselineExcludedProjectCount")
        ),
        format!(
            "vue-tsc excluded external: {}",
            number(summary, "baselineExcludedExternalCount")
        ),
        format!(
            "vue-tsc configuration errors: {}",
            number(&artifact["baseline"]["configuration"], "errorCount")
        ),
        format!(
            "vue-tsc ambient environment: {}",
            describe_ambient(&artifact["baseline"]["ambient"])
        ),
        format!("Vize Vue files: {}", number(coverage, "vizeVueFileCount")),
        format!(
            "vue-tsc Vue files: {}",
            number(coverage, "baselineVueFileCount")
        ),
        format!(
            "Shared Vue files: {}",
            number(coverage, "sharedVueFileCount")
        ),
        format!(
            "Missing Vue files: {}",
            artifact
                .pointer("/baseline/coverage/missingVueFiles")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        ),
        format!(
            "Unexpected Vue files: {}",
            artifact
                .pointer("/baseline/coverage/unexpectedVueFiles")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        ),
        format!(
            "Ignored dependency Vue files: {}",
            number(coverage, "ignoredDependencyVueFileCount")
        ),
        format!(
            "Ignored support Vue files: {}",
            number(coverage, "ignoredSupportVueFileCount")
        ),
        format!(
            "Seeded mutation oracle: {}",
            describe_mutation_oracle(&artifact["mutationOracle"])
        ),
        format!("Budget verdict: {}", describe_verdict(&artifact["budget"])),
        format!("Classification: {}", describe_classification(artifact)),
        format!(
            "Budget passed: {}",
            artifact
                .pointer("/budget/passed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        format!(
            "Digest: {}",
            artifact
                .pointer("/divergence/sha256")
                .and_then(Value::as_str)
                .unwrap_or("")
        ),
        "".to_string(),
    ]
    .join("\n")
}

fn ambient_unusable_reason(ambient: &Value) -> Option<&str> {
    if ambient.get("verdict").and_then(Value::as_str) == Some("isolated")
        && ambient.get("unusableReason").is_none_or(Value::is_null)
    {
        return None;
    }
    ambient
        .get("unusableReason")
        .and_then(Value::as_str)
        .or(Some("baseline ambient environment evidence is missing"))
}

fn mutation_oracle_unusable_reason(mutation_oracle: &Value) -> Option<&str> {
    if mutation_oracle.get("passed").and_then(Value::as_bool) == Some(true)
        && mutation_oracle.get("verdict").and_then(Value::as_str) == Some("passed")
    {
        return None;
    }
    mutation_oracle
        .get("unusableReason")
        .and_then(Value::as_str)
        .or(Some("seeded mutation oracle evidence is missing"))
}

fn diagnostic_mapping_unusable_reason(summary: &Value) -> Option<String> {
    let overlap = number(summary, "sharedCount")
        + number(summary, "messageMismatchCount")
        + number(summary, "documentedDifferenceCount");
    if overlap > 0 {
        return None;
    }
    let vize = number(summary, "vizeDiagnosticCount");
    let baseline = number(summary, "baselineDiagnosticCount");
    if vize == 0 || baseline == 0 {
        return None;
    }
    Some(format!(
        "vize reported {vize} and vue-tsc reported {baseline} diagnostics with none in common"
    ))
}

fn describe_classification(artifact: &Value) -> String {
    if artifact.pointer("/budget/verdict").and_then(Value::as_str) == Some("unusable") {
        "instrument failure, the vue-tsc baseline did not measure Vize".to_string()
    } else {
        format!(
            "Vize divergence, the vue-tsc baseline loaded cleanly over the same {} Vue files, against the fixture's own Vue runtime",
            artifact
                .pointer("/baseline/coverage/sharedVueFileCount")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        )
    }
}

fn describe_breaches(artifact: &Value) -> Vec<String> {
    let budget = &artifact["budget"];
    let summary = &artifact["divergence"]["summary"];
    let mut breaches = Vec::new();
    if budget.get("falsePositivePassed").and_then(Value::as_bool) == Some(false) {
        breaches.push(format!(
            "{} false positives (ratio {}) exceed maxFalsePositiveRatio {}",
            number(summary, "falsePositiveCount"),
            ratio_string(summary, "falsePositiveRatio"),
            ratio_string(budget, "maxFalsePositiveRatio")
        ));
    }
    if budget.get("falseNegativePassed").and_then(Value::as_bool) == Some(false) {
        breaches.push(format!(
            "{} false negatives (ratio {}) exceed maxFalseNegativeRatio {}",
            number(summary, "falseNegativeCount"),
            ratio_string(summary, "falseNegativeRatio"),
            ratio_string(budget, "maxFalseNegativeRatio")
        ));
    }
    breaches
}

fn describe_ambient(ambient: &Value) -> String {
    if ambient.is_null() {
        return "missing".to_string();
    }
    let copies = ambient
        .get("vueRuntime")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|entry| {
            Some(format!(
                "{} ×{}",
                entry.get("name").and_then(Value::as_str)?,
                entry
                    .get("copies")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)
            ))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let detail = if copies.is_empty() {
        "no Vue runtime in program".to_string()
    } else {
        copies
    };
    if let Some(reason) = ambient.get("unusableReason").and_then(Value::as_str) {
        format!(
            "{} ({reason})",
            ambient
                .get("verdict")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
    } else {
        format!(
            "{} ({detail})",
            ambient
                .get("verdict")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
    }
}

fn describe_verdict(budget: &Value) -> String {
    if let Some(reason) = budget.get("unusableReason").and_then(Value::as_str) {
        format!("unusable ({reason})")
    } else {
        budget
            .get("verdict")
            .and_then(Value::as_str)
            .unwrap_or("missing")
            .to_string()
    }
}

fn describe_mutation_oracle(mutation_oracle: &Value) -> String {
    if mutation_oracle.get("passed").and_then(Value::as_bool) == Some(true) {
        format!(
            "{} ({}:{}:{})",
            mutation_oracle
                .get("verdict")
                .and_then(Value::as_str)
                .unwrap_or("passed"),
            mutation_oracle
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("?"),
            mutation_oracle
                .pointer("/span/line")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            mutation_oracle
                .pointer("/span/column")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        )
    } else {
        format!(
            "unusable ({})",
            mutation_oracle
                .get("unusableReason")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        )
    }
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn ratio_string(value: &Value, key: &str) -> String {
    let value = value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    if value.fract() == 0.0 {
        format!("{}", value as u64)
    } else {
        format!("{value}")
    }
}

fn parse_args(argv: Vec<String>, root: &Path) -> Result<Args, String> {
    let mut args = Args {
        budget_mode: "enforce".to_string(),
        documented_differences: root.join("tests/_fixtures/compat-documented-differences.json"),
        registry: root.join("tests/_fixtures/vue-ecosystem-fixtures.json"),
        report_dir: PathBuf::new(),
        shard_count: 1,
        shard_index: 0,
        vize_bin: PathBuf::new(),
        vue_tsc_bin: PathBuf::new(),
    };
    let mut report_dir = None;
    let mut vize_bin = None;
    let mut vue_tsc_bin = None;
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        let value = |index: &mut usize| -> Result<String, String> {
            *index += 1;
            argv.get(*index)
                .cloned()
                .ok_or_else(|| format!("{arg} requires a value"))
        };
        match arg.as_str() {
            "--budget-mode" => args.budget_mode = parse_budget_mode(&value(&mut index)?)?,
            "--documented-differences" => {
                args.documented_differences = absolutize(root, PathBuf::from(value(&mut index)?))
            }
            "--registry" => args.registry = absolutize(root, PathBuf::from(value(&mut index)?)),
            "--report-dir" => {
                report_dir = Some(absolutize(root, PathBuf::from(value(&mut index)?)))
            }
            "--shard-count" => {
                args.shard_count = positive_integer(&value(&mut index)?, arg)? as usize
            }
            "--shard-index" => {
                args.shard_index = non_negative_integer(&value(&mut index)?, arg)? as usize
            }
            "--vize-bin" => vize_bin = Some(absolutize(root, PathBuf::from(value(&mut index)?))),
            "--vue-tsc-bin" => {
                vue_tsc_bin = Some(absolutize(root, PathBuf::from(value(&mut index)?)))
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => return Err(format!("Unknown argument: {arg}")),
        }
        index += 1;
    }
    args.report_dir = report_dir.ok_or_else(|| "--report-dir is required".to_string())?;
    args.vize_bin = vize_bin.ok_or_else(|| "--vize-bin is required".to_string())?;
    args.vue_tsc_bin = vue_tsc_bin.ok_or_else(|| "--vue-tsc-bin is required".to_string())?;
    if args.shard_index >= args.shard_count {
        return Err("--shard-index must be less than --shard-count".to_string());
    }
    Ok(args)
}

fn print_help() {
    println!(
        "usage: rust-script tools/commands/fixtures/typecheck-divergence-report.rs --report-dir dir --vize-bin path --vue-tsc-bin path [--registry path] [--documented-differences path] [--budget-mode enforce|record-only] [--shard-index n] [--shard-count n]"
    );
}

fn select_typecheck_projects<'a>(
    registry: &'a Value,
    args: &Args,
) -> Result<Vec<&'a Value>, String> {
    let projects = registry
        .get("projects")
        .and_then(Value::as_array)
        .ok_or_else(|| "registry must list projects".to_string())?;
    Ok(projects
        .iter()
        .enumerate()
        .filter_map(|(index, project)| {
            (index % args.shard_count == args.shard_index
                && has_enabled_typecheck_performance(project))
            .then_some(project)
        })
        .collect())
}

fn has_enabled_typecheck_performance(project: &Value) -> bool {
    project
        .pointer("/typecheckPerformance/enabled")
        .and_then(Value::as_bool)
        == Some(true)
}

fn validate_performance(project: &Value) -> Result<(), String> {
    let performance = project.get("typecheckPerformance").ok_or_else(|| {
        format!(
            "{} has no typecheckPerformance",
            project_string(project, "id").unwrap_or_else(|_| "project".to_string())
        )
    })?;
    if performance
        .get("hangTimeoutMs")
        .and_then(Value::as_u64)
        .is_none_or(|value| value == 0)
    {
        return Err(
            "typecheckPerformance.hangTimeoutMs must be a positive safe integer".to_string(),
        );
    }
    ratio_field(performance, "maxFalsePositiveRatio")?;
    ratio_field(performance, "maxFalseNegativeRatio")?;
    Ok(())
}

fn ratio_field(performance: &Value, name: &str) -> Result<f64, String> {
    let value = performance
        .get(name)
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            format!("typecheckPerformance.{name} must be a finite number between 0 and 1")
        })?;
    if (0.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "typecheckPerformance.{name} must be a finite number between 0 and 1"
        ))
    }
}

fn source_roots(
    fixture_root: &Path,
    project: &Value,
    vize_report: &Value,
) -> Result<Vec<PathBuf>, String> {
    let globs = project
        .pointer("/typecheckPerformance/corpusGlobs")
        .and_then(Value::as_array)
        .or_else(|| project.get("vueGlobs").and_then(Value::as_array));
    let mut roots = globs
        .map(|globs| {
            globs
                .iter()
                .filter_map(Value::as_str)
                .map(|glob| fixture_root.join(literal_glob_root(glob)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if roots.is_empty() {
        roots = vize_report_source_roots(fixture_root, vize_report);
    }
    dedup_paths(&mut roots);
    Ok(roots)
}

fn vize_report_source_roots(fixture_root: &Path, vize_report: &Value) -> Vec<PathBuf> {
    vize_report
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(
            vize_report
                .get("fileCount")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        )
        .filter_map(|entry| entry.get("file").and_then(Value::as_str))
        .map(|file| fixture_root.join(literal_glob_root(file)))
        .collect()
}

fn baseline_files(
    config_dir: &Path,
    fixture_root: &Path,
    vize_report: &Value,
) -> Result<Vec<String>, String> {
    let mut files = vize_report
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(
            vize_report
                .get("fileCount")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        )
        .filter_map(|entry| entry.get("file").and_then(Value::as_str))
        .map(|file| config_relative_path(config_dir, &fixture_root.join(file)))
        .collect::<Vec<_>>();
    let mut declarations = Vec::new();
    collect_ambient_declaration_files(fixture_root, &mut declarations)?;
    declarations.sort();
    declarations.dedup();
    files.extend(
        declarations
            .iter()
            .map(|path| config_relative_path(config_dir, path)),
    );
    files.sort_by(|left, right| compare_bytes(left, right));
    files.dedup();
    Ok(files)
}

fn collect_ambient_declaration_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).map_err(|error| format!("cannot read {}: {error}", dir.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if matches!(
                name.as_str(),
                "node_modules" | ".yarn" | "dist" | ".vize" | ".vize-baseline"
            ) {
                continue;
            }
            collect_ambient_declaration_files(&path, files)?;
        } else if path.is_file() && is_declaration_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_declaration_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
        })
}

fn include_globs(
    config_dir: &Path,
    ambient_roots: &[PathBuf],
    source_roots: &[PathBuf],
    dot_vue_roots: &[PathBuf],
) -> Vec<String> {
    let mut values = Vec::new();
    for root in ambient_roots {
        values.push(format!(
            "{}/**/*.d.ts",
            config_relative_path(config_dir, root)
        ));
    }
    for root in source_roots {
        let root = config_relative_path(config_dir, root);
        values.extend([
            format!("{root}/**/*.ts"),
            format!("{root}/**/*.tsx"),
            format!("{root}/**/*.mts"),
            format!("{root}/**/*.cts"),
            format!("{root}/**/*.js"),
            format!("{root}/**/*.jsx"),
            format!("{root}/**/*.mjs"),
            format!("{root}/**/*.cjs"),
            format!("{root}/**/*.json"),
        ]);
    }
    for root in dot_vue_roots {
        values.push(format!(
            "{}/**/*.vue",
            config_relative_path(config_dir, root)
        ));
    }
    values
}

fn tsconfig_include_dot_roots(
    fixture_root: &Path,
    source_dir: &Path,
    source_document: &Value,
) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::new();
    let Some(includes) = source_document.get("include").and_then(Value::as_array) else {
        return Ok(roots);
    };
    for include in includes.iter().filter_map(Value::as_str) {
        let root = source_dir.join(literal_glob_root(include));
        if !path_stays_inside(fixture_root, &root) {
            continue;
        }
        push_dot_ancestors(fixture_root, &root, &mut roots);
        collect_dot_directories(&root, &mut roots)?;
    }
    Ok(roots)
}

fn push_dot_ancestors(fixture_root: &Path, path: &Path, roots: &mut Vec<PathBuf>) {
    let Some(relative) = pathdiff::diff_paths(path, fixture_root) else {
        return;
    };
    let mut current = fixture_root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return;
        };
        current.push(segment);
        if is_dot_directory(&segment.to_string_lossy()) {
            roots.push(current.clone());
        }
    }
}

fn dot_directory_include_roots(fixture_root: &Path, vize_report: &Value) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for file in vize_report
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(
            vize_report
                .get("fileCount")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        )
        .filter_map(|entry| entry.get("file").and_then(Value::as_str))
    {
        let normalized = file.replace('\\', "/");
        let segments = normalized.split('/').collect::<Vec<_>>();
        for index in 0..segments.len().saturating_sub(1) {
            if !is_dot_directory(segments[index])
                || has_ancestor_segment(&segments, index, "node_modules")
            {
                continue;
            }
            roots.push(
                segments[..=index]
                    .iter()
                    .fold(fixture_root.to_path_buf(), |path, segment| {
                        path.join(segment)
                    }),
            );
        }
    }
    roots
}

fn has_ancestor_segment(segments: &[&str], end: usize, expected: &str) -> bool {
    segments
        .iter()
        .take(end)
        .any(|segment| *segment == expected)
}

fn discover_dot_directory_include_roots(source_roots: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::new();
    for source_root in source_roots {
        collect_dot_directories(source_root, &mut roots)?;
    }
    Ok(roots)
}

fn collect_dot_directories(directory: &Path, roots: &mut Vec<PathBuf>) -> Result<(), String> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            continue;
        }
        if ignored_directory(&name) {
            continue;
        }
        let child = entry.path();
        if is_dot_directory(&name) {
            roots.push(child.clone());
        }
        collect_dot_directories(&child, roots)?;
    }
    Ok(())
}

fn ignored_directory(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | "dist" | ".git" | ".generated" | ".vize-baseline" | ".yarn"
    )
}

fn read_tsconfig_jsonc(path: &Path) -> Result<Value, String> {
    let source = common::read_text(path)?;
    serde_json::from_str(&source)
        .or_else(|_| serde_json::from_str(&strip_jsonc_sugar(&source)))
        .map_err(|error| format!("cannot parse JSON {}: {error}", path.display()))
}

struct CompilerPaths {
    paths: Option<DeclaredPaths>,
    base_url: Option<DeclaredBaseUrl>,
}

struct DeclaredPaths {
    dir: PathBuf,
    value: serde_json::Map<String, Value>,
}

struct DeclaredBaseUrl {
    dir: PathBuf,
    value: String,
}

fn winning_compiler_paths(
    fixture_root: &Path,
    source_path: &Path,
) -> Result<CompilerPaths, String> {
    let mut chain = Vec::new();
    collect_tsconfig_extends_chain(fixture_root, source_path, &mut BTreeSet::new(), &mut chain)?;
    let mut paths = None;
    let mut base_url = None;
    for (path, document) in chain {
        let dir = path
            .parent()
            .ok_or_else(|| format!("tsconfig has no parent: {}", path.display()))?
            .to_path_buf();
        if let Some(value) = document
            .pointer("/compilerOptions/paths")
            .and_then(Value::as_object)
        {
            paths = Some(DeclaredPaths {
                dir: dir.clone(),
                value: value.clone(),
            });
        }
        if let Some(value) = document
            .pointer("/compilerOptions/baseUrl")
            .and_then(Value::as_str)
        {
            base_url = Some(DeclaredBaseUrl {
                dir,
                value: value.to_string(),
            });
        }
    }
    Ok(CompilerPaths { paths, base_url })
}

fn collect_tsconfig_extends_chain(
    fixture_root: &Path,
    path: &Path,
    seen: &mut BTreeSet<PathBuf>,
    chain: &mut Vec<(PathBuf, Value)>,
) -> Result<(), String> {
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(normalized.clone()) {
        return Ok(());
    }
    let document = read_tsconfig_jsonc(&normalized)?;
    if let Some(parent) = document
        .get("extends")
        .and_then(Value::as_str)
        .and_then(|extends| resolve_relative_tsconfig_extends(fixture_root, &normalized, extends))
    {
        collect_tsconfig_extends_chain(fixture_root, &parent, seen, chain)?;
    }
    chain.push((normalized, document));
    Ok(())
}

fn resolve_relative_tsconfig_extends(
    fixture_root: &Path,
    from_config: &Path,
    specifier: &str,
) -> Option<PathBuf> {
    if !(specifier.starts_with("./") || specifier.starts_with("../")) {
        return None;
    }
    let base = from_config.parent()?.join(specifier);
    let candidates = if base.extension().is_some() {
        vec![base]
    } else {
        vec![base.with_extension("json"), base]
    };
    candidates.into_iter().find(|candidate| {
        candidate.exists()
            && candidate
                .canonicalize()
                .ok()
                .is_some_and(|path| path_stays_inside(fixture_root, &path))
    })
}

fn strip_jsonc_sugar(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
            }
            '}' | ']' => {
                while out.ends_with(char::is_whitespace) {
                    out.pop();
                }
                if out.ends_with(',') {
                    out.pop();
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn is_dot_directory(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

fn source_path_mappings(
    source_dir: &Path,
    config_dir: &Path,
    declared: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    declared
        .iter()
        .filter_map(|(name, targets)| {
            let targets = targets
                .as_array()?
                .iter()
                .map(|target| {
                    target
                        .as_str()
                        .map(|target| json!(relocate_path_mapping(source_dir, config_dir, target)))
                        .unwrap_or_else(|| target.clone())
                })
                .collect::<Vec<_>>();
            Some((name.clone(), Value::Array(targets)))
        })
        .collect()
}

fn relocate_path_mapping(source_dir: &Path, config_dir: &Path, target: &str) -> String {
    if target == "*" {
        return format!("{}/*", config_relative_path(config_dir, source_dir));
    }
    if target.ends_with("/*") && !target[..target.len() - 2].contains('*') {
        return format!(
            "{}/*",
            config_relative_path(config_dir, &source_dir.join(&target[..target.len() - 2]))
        );
    }
    if !target.contains('*') {
        return config_relative_path(config_dir, &source_dir.join(target));
    }
    target.to_string()
}

fn extend_local_vue_runtime_paths(
    fixture_root: &Path,
    config_dir: &Path,
    paths: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let runtime_names = ["@vue/runtime-core", "@vue/runtime-dom", "vue"];
    let vue_root = local_package_root(fixture_root, "vue")?;
    for name in runtime_names {
        let root = if name == "vue" {
            vue_root.clone()
        } else {
            match local_vue_dependency_package_root(fixture_root, vue_root.as_deref(), name)? {
                Some(root) => Some(root),
                None => local_package_root(fixture_root, name)?,
            }
        };
        if let Some(root) = root {
            let root = config_relative_path(config_dir, &root);
            paths.insert(name.to_string(), json!([root.clone()]));
            paths.insert(format!("{name}/*"), json!([format!("{root}/*")]));
        }
    }
    Ok(())
}

fn local_vue_dependency_package_root(
    fixture_root: &Path,
    vue_root: Option<&Path>,
    name: &str,
) -> Result<Option<PathBuf>, String> {
    if name == "vue" {
        return Ok(None);
    }
    let Some(vue_root) = vue_root else {
        return Ok(None);
    };
    let real_vue_root = fs::canonicalize(vue_root)
        .map_err(|error| format!("cannot resolve {}: {error}", vue_root.display()))?;
    let Some(store_node_modules) = real_vue_root.parent() else {
        return Ok(None);
    };
    let candidate = name
        .split('/')
        .fold(store_node_modules.to_path_buf(), |path, segment| {
            path.join(segment)
        });
    if !candidate.join("package.json").exists() {
        return Ok(None);
    }
    let real = fs::canonicalize(&candidate)
        .map_err(|error| format!("cannot resolve {}: {error}", candidate.display()))?;
    if !path_stays_inside(fixture_root, &real) {
        return Ok(None);
    }
    Ok(Some(candidate))
}

fn local_package_root(fixture_root: &Path, name: &str) -> Result<Option<PathBuf>, String> {
    let linked = name
        .split('/')
        .fold(fixture_root.join("node_modules"), |path, segment| {
            path.join(segment)
        });
    if linked.join("package.json").exists() {
        return Ok(Some(linked));
    }
    unique_pnpm_store_package_root(fixture_root, name)
}

fn unique_pnpm_store_package_root(
    fixture_root: &Path,
    name: &str,
) -> Result<Option<PathBuf>, String> {
    let store = fixture_root.join("node_modules/.pnpm");
    if !store.exists() {
        return Ok(None);
    }
    let mut matches = BTreeMap::new();
    for entry in
        fs::read_dir(&store).map_err(|error| format!("cannot read {}: {error}", store.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let entry_name = entry.file_name().to_string_lossy().to_string();
        if entry_name.starts_with('.') {
            continue;
        }
        if !pnpm_store_entry_matches_package(&entry_name, name) {
            continue;
        }
        let candidate = name
            .split('/')
            .fold(entry.path().join("node_modules"), |path, segment| {
                path.join(segment)
            });
        if !candidate.join("package.json").exists() {
            continue;
        }
        let real = fs::canonicalize(&candidate)
            .map_err(|error| format!("cannot resolve {}: {error}", candidate.display()))?;
        if !path_stays_inside(fixture_root, &real) {
            continue;
        }
        matches.insert(real, candidate);
    }
    Ok(if matches.len() == 1 {
        matches.into_values().next()
    } else {
        None
    })
}

fn pnpm_store_entry_matches_package(entry_name: &str, name: &str) -> bool {
    let encoded = name.replace('/', "+");
    entry_name
        .strip_prefix(&encoded)
        .is_some_and(|suffix| suffix.starts_with('@'))
}

fn path_stays_inside(root: &Path, path: &Path) -> bool {
    pathdiff::diff_paths(path, root).is_some_and(|relative| {
        relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    })
}

fn literal_glob_root(glob: &str) -> &str {
    let magic = glob
        .find(['*', '?', '[', ']', '{', '}'])
        .unwrap_or(glob.len());
    let prefix = &glob[..magic];
    match prefix.rfind('/') {
        Some(index) if index > 0 => &prefix[..index],
        _ => ".",
    }
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    paths.sort();
    paths.dedup();
}

fn config_relative_path(from: &Path, to: &Path) -> String {
    let path = pathdiff::diff_paths(to, from).unwrap_or_else(|| to.to_path_buf());
    let normalized = common::normalize_path(&path);
    if normalized.starts_with('.') || normalized.starts_with('/') {
        normalized
    } else {
        format!("./{normalized}")
    }
}

fn file_list_hash(files: &[String]) -> String {
    sha256(
        &files
            .iter()
            .map(|file| format!("{file}\n"))
            .collect::<String>(),
    )
}

fn parse_budget_mode(value: &str) -> Result<String, String> {
    if value == "enforce" || value == "record-only" {
        Ok(value.to_string())
    } else {
        Err("--budget-mode must be one of: enforce, record-only".to_string())
    }
}

fn positive_integer(value: &str, name: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive integer"))
}

fn non_negative_integer(value: &str, name: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{name} must be a non-negative integer"))
}

fn absolutize(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn project_string(project: &Value, field: &str) -> Result<String, String> {
    project
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("project is missing {field}"))
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        canonical_json(&map[key])
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        _ => serde_json::to_string(value).unwrap(),
    }
}

fn sha256(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}

fn sha256_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
