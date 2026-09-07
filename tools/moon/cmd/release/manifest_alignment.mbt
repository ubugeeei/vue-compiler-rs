/// Node runner used for release-side JS checks.
fn release_node_bin() -> String {
  match @env.get_env_var("VIZE_RELEASE_NODE") {
    Some(value) if value != "" => value
    _ => "node"
  }
}

/// Runs the manifest alignment test before release files are mutated.
async fn verify_manifest_alignment_before_mutation() -> Bool {
  println("Verifying manifest alignment before repository mutation...")
  if @process.run(
    release_node_bin(),
    ["--test", "tests/tooling/package-manifests.test.ts"],
  ) != 0 {
    @stdio.stderr.write(
      "Error: package manifest alignment tests failed before repository mutation. Install JS dependencies or fix the manifest drift, then retry.\n",
    )
    return false
  }
  true
}

/// Runs the manifest alignment test against the generated release commit.
async fn verify_manifest_alignment_after_commit() -> Bool {
  println("Verifying manifest alignment with tests/tooling/package-manifests.test.ts...")
  if @process.run(
    release_node_bin(),
    ["--test", "tests/tooling/package-manifests.test.ts"],
  ) != 0 {
    @stdio.stderr.write(
      "Error: package manifest alignment tests failed against the release commit. Inspect tests/tooling/package-manifests.test.ts, then run `git reset --hard origin/main` after confirming there is no work to keep, fix the drift, and retry.\n",
    )
    return false
  }
  true
}
