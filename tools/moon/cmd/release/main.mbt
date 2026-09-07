/// Prepares a release commit and tag from the repository root.
/// This script is intentionally conservative because it mutates versioned files,
/// creates a git commit, and pushes a release tag that fans out into the
/// publishing workflow. Its job is to keep all version bumps reproducible and
/// explicit: it validates the working tree, computes the next version from the
/// requested bump kind, updates every tracked manifest that must stay aligned,
/// and only then creates and pushes the release commit and tag.

/// Parsed semantic version components used by the release bump logic.
/// The struct stores only the fields the repository needs today: major / minor /
/// patch plus an optional prerelease label and numeric suffix.
struct Version {
  major : Int
  minor : Int
  patch : Int
  prerelease : String?
  prerelease_num : Int?
}

async fn is_git_ignored(path : String) -> Bool {
  @process.run("git", ["check-ignore", "-q", path]) == 0
}

/// Writes a text file and converts write failures into a boolean result.
fn write_string_to_file(path : String, content : String) -> Bool {
  try {
    @xfs.write_string_to_file(path, content)
    true
  } catch {
    _ => false
  }
}

/// Parses an integer but keeps the caller in control of error handling.
fn parse_int(value : String) -> Int? {
  try {
    Some(@string.parse_int(value))
  } catch {
    _ => None
  }
}

/// Parses the repository's release version format into structured fields.
/// Supported inputs are:
/// - `major.minor.patch`
/// - `major.minor.patch-label`
/// - `major.minor.patch-label.number`
/// The parser is intentionally strict so release automation never guesses when
/// the current version format has drifted unexpectedly.
fn parse_version(value : String) -> Version? {
  let parts : Array[String] = value.split("-").map(view => view.to_owned()).collect()
  guard parts.length() >= 1 && parts.length() <= 2 else { return None }

  let base_parts : Array[String] = parts[0].split(".").map(view => view.to_owned()).collect()
  guard base_parts.length() == 3 else { return None }
  let major = match parse_int(base_parts[0]) {
    Some(value) => value
    None => return None
  }
  let minor = match parse_int(base_parts[1]) {
    Some(value) => value
    None => return None
  }
  let patch = match parse_int(base_parts[2]) {
    Some(value) => value
    None => return None
  }

  let mut prerelease : String? = None
  let mut prerelease_num : Int? = None
  if parts.length() == 2 {
    let prerelease_parts : Array[String] = parts[1]
      .split(".")
      .map(view => view.to_owned())
      .collect()
    guard prerelease_parts.length() >= 1 && prerelease_parts.length() <= 2 else {
      return None
    }
    guard prerelease_parts[0] != "" else { return None }
    prerelease = Some(prerelease_parts[0])
    if prerelease_parts.length() == 2 {
      prerelease_num = match parse_int(prerelease_parts[1]) {
        Some(value) => Some(value)
        None => return None
      }
    }
  }

  Some(Version::{
    major,
    minor,
    patch,
    prerelease,
    prerelease_num,
  })
}

/// Formats a structured version back into the repository's release string form.
fn format_version(
  major : Int,
  minor : Int,
  patch : Int,
  prerelease : String?,
  prerelease_num : Int?,
) -> String {
  let base = "\{major}.\{minor}.\{patch}"
  match prerelease {
    None => base
    Some(label) =>
      match prerelease_num {
        Some(number) => "\{base}-\{label}.\{number}"
        None => "\{base}-\{label}"
      }
  }
}

/// Computes the next release version for a given bump strategy.
/// Stable bumps (`patch`, `minor`, `major`) reset the numeric suffix, while
/// prerelease bumps preserve the existing base version and increment the suffix
/// only when the current version already uses the same prerelease channel.
fn bump_version(current : String, bump : String) -> String? {
  let version = match parse_version(current) {
    Some(version) => version
    None => return None
  }
  match bump {
    "patch" =>
      Some(
        format_version(
          version.major,
          version.minor,
          version.patch + 1,
          None,
          None,
        ),
      )
    "minor" =>
      Some(
        format_version(
          version.major,
          version.minor + 1,
          0,
          None,
          None,
        ),
      )
    "major" =>
      Some(format_version(version.major + 1, 0, 0, None, None))
    "alpha" =>
      if version.prerelease == Some("alpha") {
        Some(
          format_version(
            version.major,
            version.minor,
            version.patch,
            Some("alpha"),
            Some(version.prerelease_num.unwrap_or(1) + 1),
          ),
        )
      } else {
        Some(
          format_version(
            version.major,
            version.minor,
            version.patch,
            Some("alpha"),
            None,
          ),
        )
      }
    "beta" =>
      Some(
        format_version(
          version.major,
          version.minor,
          version.patch,
          Some("beta"),
          None,
        ),
      )
    "rc" =>
      Some(format_version(version.major, version.minor, version.patch, Some("rc"), None))
    "release" => Some(format_version(version.major, version.minor, version.patch, None, None))
    _ => None
  }
}

/// Extracts the current workspace version from the root `Cargo.toml`.
fn workspace_version_from_content(content : String) -> String? {
  let mut section = ""
  let lines : Array[String] = content.split("\n").map(view => view.to_owned()).collect()
  for line in lines {
    let trimmed = line.trim()
    if trimmed.has_prefix("[") && trimmed.has_suffix("]") {
      section = trimmed.to_owned()
      continue
    }
    if section == "[workspace.package]" &&
      trimmed.has_prefix("version = \"") &&
      trimmed.has_suffix("\"") {
      return Some(
        trimmed
          .to_owned()
          .replace(old="version = \"", new="")
          .replace_all(old="\"", new=""),
      )
    }
  }
  None
}

/// Rewrites all workspace-level Rust version pins that must stay aligned.
/// This updates both `[workspace.package].version` and the exact-version
/// `vize_*` dependencies inside `[workspace.dependencies]`.
fn update_workspace_manifest_versions(
  content : String,
  current_version : String,
  new_version : String,
) -> String {
  let mut section = ""
  let updated : Array[String] = []
  let lines : Array[String] = content.split("\n").map(view => view.to_owned()).collect()
  for line in lines {
    let trimmed = line.trim()
    if trimmed.has_prefix("[") && trimmed.has_suffix("]") {
      section = trimmed.to_owned()
      updated.push(line)
      continue
    }
    if section == "[workspace.package]" && trimmed == "version = \"\{current_version}\"" {
      updated.push(line.replace(old=current_version, new=new_version))
      continue
    }
    if section == "[workspace.dependencies]" &&
      trimmed.has_prefix("vize_") &&
      line.contains("version = \"=\{current_version}\"") {
      updated.push(
        line.replace_all(old="version = \"=\{current_version}\"", new="version = \"=\{new_version}\""),
      )
      continue
    }
    updated.push(line)
  }
  updated.join("\n")
}

/// Replaces the first package.json `"version"` field that matches the current release.
fn replace_package_json_version(
  content : String,
  current_version : String,
  new_version : String,
) -> String {
  let updated : Array[String] = []
  let mut replaced = false
  let lines : Array[String] = content.split("\n").map(view => view.to_owned()).collect()
  for line in lines {
    if !replaced {
      let trimmed = line.trim()
      if trimmed.has_prefix("\"version\"") && line.contains(current_version) {
        updated.push(line.replace(old=current_version, new=new_version))
        replaced = true
        continue
      }
    }
    updated.push(line)
  }
  updated.join("\n")
}

/// Nested npm group directories (npm/builder, npm/framework, npm/compose,
/// npm/ui, ...) hold their packages one level deeper than the flat
/// npm/<package> layout. Scanning them keeps every new group aligned with the
/// release version without editing a hardcoded list — the release preflight
/// verifies all tracked non-private manifests, so any package this scan
/// misses fails the release before publication.
async fn nested_npm_package_dirs() -> Array[String] {
  let package_dirs : Array[String] = []
  for group_name in @tool.read_dir("npm") {
    let group_dir = @tool.join_path("npm", group_name)
    if !@tool.is_dir(group_dir) {
      continue
    }
    if @tool.path_exists(@tool.join_path(group_dir, "package.json")) {
      continue
    }
    for entry_name in @tool.read_dir(group_dir) {
      let package_dir = @tool.join_path(group_dir, entry_name)
      if !@tool.is_dir(package_dir) {
        continue
      }
      let package_json_path = @tool.join_path(package_dir, "package.json")
      if !@tool.path_exists(package_json_path) {
        continue
      }
      // Mirror the release preflight exactly: it verifies tracked,
      // non-private manifests, so ignored build output and private
      // packages stay out of the release version sweep.
      if is_git_ignored(package_json_path) {
        continue
      }
      match @tool.read_file_to_string(package_json_path) {
        Some(content) =>
          if content.contains("\"private\": true") {
            continue
          }
        None => continue
      }
      package_dirs.push(package_dir)
    }
  }
  package_dirs.sort_by((a, b) => a.lexical_compare(b))
  package_dirs
}

/// Package manifests that are intentionally outside the npm workspace package
/// globs but still ship release artifacts and must stay aligned with the
/// workspace version.
async fn extra_release_package_json_paths() -> Array[String] {
  let paths : Array[String] = ["editors/vscode/package.json", "editors/vscode-art/package.json"]
  for package_dir in nested_npm_package_dirs() {
    paths.push(@tool.join_path(package_dir, "package.json"))
  }
  paths
}

/// Replaces the first TOML `version = "..."` line in a file.
/// This is used for the Zed extension manifests where a full TOML parser would
/// be overkill for the repository's simple version synchronization needs.
fn replace_first_version_line(
  content : String,
  current_version : String,
  new_version : String,
) -> String {
  let updated : Array[String] = []
  let mut replaced = false
  let lines : Array[String] = content.split("\n").map(view => view.to_owned()).collect()
  for line in lines {
    if !replaced {
      let trimmed = line.trim()
      if trimmed.has_prefix("version = \"") &&
        trimmed.has_suffix("\"") &&
        line.contains(current_version) {
        updated.push(line.replace(old=current_version, new=new_version))
        replaced = true
        continue
      }
    }
    updated.push(line)
  }
  updated.join("\n")
}

/// Replaces every occurrence of the old version string in a text file.
fn replace_all_version_references(
  content : String,
  current_version : String,
  new_version : String,
) -> String {
  content.replace_all(old=current_version, new=new_version)
}

/// Detects whether the script can safely prompt the user for confirmation.
/// The release script is allowed to be interactive when run locally, but CI and
/// sessions without a controlling terminal must fail explicitly instead of
/// hanging on stdin reads.
async fn has_interactive_terminal() -> Bool {
  @process.run("sh", ["-c", ": </dev/tty >/dev/tty"]) == 0
}

/// Prompts through the controlling terminal because task runners keep stdin.
async fn confirm_release() -> Bool {
  let command = "printf 'Proceed with release? [y/N] ' > /dev/tty; IFS= read -r answer < /dev/tty; case \"$answer\" in y|Y|yes|Yes|YES) exit 0 ;; *) exit 1 ;; esac"
  @process.run("sh", ["-c", command]) == 0
}

/// Filters a list of candidate paths down to the ones that currently exist.
/// The release commit stages files opportunistically because some package
/// variants are optional in this repository.
fn stage_files(files : Array[String]) -> Array[String] {
  let staged = []
  for file in files {
    if @tool.path_exists(file) {
      staged.push(file)
    }
  }
  staged
}

/// Runs the shared immutable-tag and current-main guard before any mutation.
/// Tests may override the script path while exercising isolated repositories.
async fn run_local_release_guard(tag : String) -> Bool {
  let script = match @env.get_env_var(
    "VIZE_RELEASE_GUARD_SCRIPT",
  ) {
    Some(value) if value != "" => value
    _ => "tools/commands/ci/github/release-local-guard.rs"
  }
  if !@tool.path_exists(script) {
    @stdio.stderr.write("Release preflight script not found: " + script + "\n")
    return false
  }

  let runner = match @env.get_env_var("VIZE_RELEASE_GUARD_RUNNER") {
    Some(value) if value != "" => value
    _ => if script.has_suffix(".mjs") { "node" } else { "rust-script" }
  }
  let exit_code = @process.run(runner, [script, tag])
  if exit_code != 0 {
    @stdio.stderr.write(
      "Release preflight failed before repository mutation.\n",
    )
    return false
  }

  true
}

/// Main entry point for release preparation and publication kickoff.
/// The function computes the next version, confirms the operation when running
/// interactively, verifies the repository is in a publishable state, updates
/// all synchronized manifests, creates the release commit, tags it, and pushes
/// the branch and immutable tag atomically so the tag-triggered publishing
/// workflow can take over without exposing a partially-updated release.
async fn run_app() -> Int {
  let args = @tool.cli_args()
  if args.length() == 3 && args[0] == "--print-bump" {
    match bump_version(args[1], args[2]) {
      Some(version) => {
        println(version)
        return 0
      }
      None => return 1
    }
  }
  if args.length() == 4 && args[0] == "--print-workspace-catalog-update" {
    let input = match @tool.read_file_to_string(args[1]) {
      Some(text) => text
      None => {
        @stdio.stderr.write("Failed to read \{args[1]}\n")
        return 1
      }
    }
    @stdio.stdout.write(
      replace_native_binary_workspace_catalog(input, args[2], args[3]),
    )
    return 0
  }
  if args.length() == 4 && args[0] == "--print-lockfile-catalog-update" {
    let input = match @tool.read_file_to_string(args[1]) {
      Some(text) => text
      None => {
        @stdio.stderr.write("Failed to read \{args[1]}\n")
        return 1
      }
    }
    @stdio.stdout.write(
      replace_native_binary_lockfile_catalog(input, args[2], args[3]),
    )
    return 0
  }
  if args.length() == 1 && args[0] == "--print-extra-package-json-paths" {
    for path in extra_release_package_json_paths() {
      println(path)
    }
    return 0
  }
  if args.is_empty() {
    @stdio.stderr.write(
      "Usage: moon run --target native tools/moon/cmd/release -- <patch|minor|major|alpha|beta|rc|release> [-y]\n",
    )
    return 1
  }

  let bump = args[0]
  let mut auto_confirm = false
  for i in 1..<args.length() {
    if args[i] == "-y" {
      auto_confirm = true
      continue
    }
    if args[i] == "--force-tag" {
      @stdio.stderr.write(
        "Error: --force-tag is not supported; published release tags are immutable.\n",
      )
      return 1
    }
    if args[i] != "-y" {
      @stdio.stderr.write("Unknown release arguments\n")
      return 1
    }
  }

  let cargo_toml_path = "Cargo.toml"
  let cargo_toml = match @tool.read_file_to_string(cargo_toml_path) {
    Some(content) => content
    None => {
      @stdio.stderr.write("Failed to read Cargo.toml\n")
      return 1
    }
  }
  let current_version = match workspace_version_from_content(cargo_toml) {
    Some(version) => version
    None => {
      @stdio.stderr.write("Failed to read current version from Cargo.toml\n")
      return 1
    }
  }
  let new_version = match bump_version(current_version, bump) {
    Some(version) => version
    None => {
      @stdio.stderr.write("Failed to compute the next version\n")
      return 1
    }
  }
  let tag = "v" + new_version

  println("Current version: \{current_version}")
  println("New version: \{new_version} (tag: \{tag})")
  println("")

  if !auto_confirm {
    if !has_interactive_terminal() {
      @stdio.stderr.write(
        "Error: Confirmation requires an interactive terminal. Re-run with -y to skip the prompt.\n",
      )
      return 1
    }
    if !confirm_release() {
      println("Aborted.")
      return 1
    }
  }

  if !run_local_release_guard(tag) {
    return 1
  }

  if !verify_manifest_alignment_before_mutation() {
    return 1
  }

  println("Updating Cargo.toml...")
  if !write_string_to_file(
    cargo_toml_path,
    update_workspace_manifest_versions(cargo_toml, current_version, new_version),
  ) {
    @stdio.stderr.write("Failed to update Cargo.toml\n")
    return 1
  }

  println("Updating Cargo.lock...")
  if @process.run("cargo", ["update", "--workspace"]) != 0 {
    @stdio.stderr.write("Failed to update Cargo.lock\n")
    return 1
  }

  let files_to_stage = ["Cargo.toml", "Cargo.lock"]
  println("Updating npm packages...")
  for entry_name in @tool.read_dir("npm") {
    let package_dir = @tool.join_path("npm", entry_name)
    if !@tool.is_dir(package_dir) {
      continue
    }
    let package_json_path = @tool.join_path(package_dir, "package.json")
    if @tool.path_exists(package_json_path) {
      let package_json = match @tool.read_file_to_string(package_json_path) {
        Some(content) => content
        None => {
          @stdio.stderr.write("Failed to read \{package_json_path}\n")
          return 1
        }
      }
      if !write_string_to_file(
        package_json_path,
        replace_package_json_version(package_json, current_version, new_version),
      ) {
        @stdio.stderr.write("Failed to update \{package_json_path}\n")
        return 1
      }
      println("  Updated \{package_json_path}")
      files_to_stage.push(package_json_path)
    }
  }
  for package_json_path in extra_release_package_json_paths() {
    if !@tool.path_exists(package_json_path) {
      continue
    }
    let package_json = match @tool.read_file_to_string(package_json_path) {
      Some(content) => content
      None => {
        @stdio.stderr.write("Failed to read \{package_json_path}\n")
        return 1
      }
    }
    if !write_string_to_file(
      package_json_path,
      replace_package_json_version(package_json, current_version, new_version),
    ) {
      @stdio.stderr.write("Failed to update \{package_json_path}\n")
      return 1
    }
    println("  Updated \{package_json_path}")
    files_to_stage.push(package_json_path)
  }

  let workspace_yaml_path = "pnpm-workspace.yaml"
  let workspace_yaml = match @tool.read_file_to_string(workspace_yaml_path) {
    Some(content) => content
    None => {
      @stdio.stderr.write("Failed to read \{workspace_yaml_path}\n")
      return 1
    }
  }
  let updated_workspace_yaml = replace_native_binary_workspace_catalog(
    workspace_yaml,
    current_version,
    new_version,
  )
  if updated_workspace_yaml != workspace_yaml {
    if !write_string_to_file(workspace_yaml_path, updated_workspace_yaml) {
      @stdio.stderr.write("Failed to update \{workspace_yaml_path}\n")
      return 1
    }
    println("  Updated \{workspace_yaml_path}")
    files_to_stage.push(workspace_yaml_path)
  }

  let lockfile_path = "pnpm-lock.yaml"
  let lockfile = match @tool.read_file_to_string(lockfile_path) {
    Some(content) => content
    None => {
      @stdio.stderr.write("Failed to read \{lockfile_path}\n")
      return 1
    }
  }
  let updated_lockfile = replace_native_binary_lockfile_catalog(
    lockfile,
    current_version,
    new_version,
  )
  if updated_lockfile != lockfile {
    if !write_string_to_file(lockfile_path, updated_lockfile) {
      @stdio.stderr.write("Failed to update \{lockfile_path}\n")
      return 1
    }
    println("  Updated \{lockfile_path} native-binaries catalog block")
    files_to_stage.push(lockfile_path)
  }

  let zed_cargo_toml = "editors/zed/Cargo.toml"
  let zed_extension_toml = "editors/zed/extension.toml"
  let zed_cargo_lock = "editors/zed/Cargo.lock"
  if @tool.path_exists(zed_cargo_toml) {
    let content = match @tool.read_file_to_string(zed_cargo_toml) {
      Some(content) => content
      None => {
        @stdio.stderr.write("Failed to read \{zed_cargo_toml}\n")
        return 1
      }
    }
    if !write_string_to_file(
      zed_cargo_toml,
      replace_first_version_line(content, current_version, new_version),
    ) {
      @stdio.stderr.write("Failed to update \{zed_cargo_toml}\n")
      return 1
    }
    println("  Updated \{zed_cargo_toml}")
    files_to_stage.push(zed_cargo_toml)
  }
  if @tool.path_exists(zed_extension_toml) {
    let content = match @tool.read_file_to_string(zed_extension_toml) {
      Some(content) => content
      None => {
        @stdio.stderr.write("Failed to read \{zed_extension_toml}\n")
        return 1
      }
    }
    if !write_string_to_file(
      zed_extension_toml,
      replace_first_version_line(content, current_version, new_version),
    ) {
      @stdio.stderr.write("Failed to update \{zed_extension_toml}\n")
      return 1
    }
    println("  Updated \{zed_extension_toml}")
    files_to_stage.push(zed_extension_toml)
  }
  if @tool.path_exists(zed_cargo_lock) {
    if @process.run(
      "cargo",
      ["update", "--manifest-path", "editors/zed/Cargo.toml", "-p", "vize-zed-extension"],
    ) != 0 {
      @stdio.stderr.write("Failed to update editors/zed/Cargo.lock\n")
      return 1
    }
    println("  Updated \{zed_cargo_lock}")
    files_to_stage.push(zed_cargo_lock)
  }

  println("Updating READMEs...")
  let root_readme = "README.md"
  if @tool.path_exists(root_readme) && !is_git_ignored(root_readme) {
    let content = match @tool.read_file_to_string(root_readme) {
      Some(content) => content
      None => {
        @stdio.stderr.write("Failed to read \{root_readme}\n")
        return 1
      }
    }
    if !write_string_to_file(
      root_readme,
      replace_all_version_references(content, current_version, new_version),
    ) {
      @stdio.stderr.write("Failed to update \{root_readme}\n")
      return 1
    }
    files_to_stage.push(root_readme)
  }
  let readme_package_dirs : Array[String] = []
  for entry_name in @tool.read_dir("npm") {
    let package_dir = @tool.join_path("npm", entry_name)
    if !@tool.is_dir(package_dir) {
      continue
    }
    readme_package_dirs.push(package_dir)
  }
  for package_dir in nested_npm_package_dirs() {
    readme_package_dirs.push(package_dir)
  }
  for package_dir in readme_package_dirs {
    let readme_path = @tool.join_path(package_dir, "README.md")
    if !@tool.path_exists(readme_path) {
      continue
    }
    if is_git_ignored(readme_path) {
      continue
    }
    let content = match @tool.read_file_to_string(readme_path) {
      Some(content) => content
      None => {
        @stdio.stderr.write("Failed to read \{readme_path}\n")
        return 1
      }
    }
    if !write_string_to_file(
      readme_path,
      replace_all_version_references(content, current_version, new_version),
    ) {
      @stdio.stderr.write("Failed to update \{readme_path}\n")
      return 1
    }
    files_to_stage.push(readme_path)
  }

  println("Committing changes...")
  let git_add_args = ["add"]
  for file in stage_files(files_to_stage) {
    git_add_args.push(file)
  }
  if @process.run("git", git_add_args) != 0 {
    @stdio.stderr.write("Failed to stage release files\n")
    return 1
  }

  let (staged_exit, staged_output) = @process.collect_stdout(
    "git",
    ["diff", "--cached", "--name-only"],
  )
  if staged_exit != 0 {
    @stdio.stderr.write("Failed to inspect staged release files\n")
    return 1
  }
  if staged_output.text().trim() == "" {
    @stdio.stderr.write("No release changes were staged; refusing to create an empty release.\n")
    return 1
  }
  let commit_args = ["commit", "--no-verify", "-m", "chore: release \{tag}"]
  if @process.run("git", commit_args) != 0 {
    @stdio.stderr.write("Failed to create the release commit\n")
    return 1
  }

  if !verify_manifest_alignment_after_commit() {
    return 1
  }

  println("Creating tag \{tag}...")
  let git_tag_args = ["tag", "-a", tag, "-m", "Release \{new_version}"]
  if @process.run("git", git_tag_args) != 0 {
    @stdio.stderr.write("Failed to create the git tag\n")
    return 1
  }

  println("Pushing main and tag atomically...")
  if @process.run(
    "git",
    ["push", "--atomic", "origin", "main", "refs/tags/" + tag],
  ) != 0 {
    @stdio.stderr.write("Failed to atomically push main and the release tag\nRemote refs were unchanged. Before retrying, run `git tag -d \{tag}`, fetch origin/main, and remove the generated release commit (for example, `git reset --hard origin/main` after confirming there is no work to keep).\n")
    return 1
  }

  println("")
  println("Release \{new_version} complete!")
  println("GitHub Actions will now publish to npm and crates.io.")
  println("Check: https://github.com/ubugeeei-prod/vize/actions")
  0
}

async fn main {
  @sys.exit(run_app())
}
