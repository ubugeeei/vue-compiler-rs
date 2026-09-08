import assert from "node:assert/strict";
import { test } from "node:test";

import { parse } from "yaml";

import { readRepoFile, workflowJobBody } from "./support/github-workflows.ts";

const UPSTREAM_SHA = "d6c4afddb2c55f4a9dea7b59293a99a8fdea1799";
const CONTENT_MAPPER_PROTOCOL_COMMAND =
  "go test ./internal/contentmapper -run '^(TestRunnerTransform|TestRunnerTransformResponseValidation)$' -count=1";
const SPANMAP_PROTOCOL_COMMAND =
  "go test ./internal/spanmap -run '^(TestOriginalToVirtualOverlappingSpans|TestValidateOriginalOverlapAndFeatures)$' -count=1";
const TSGO_BUILD_COMMAND = 'go build -tags=noembed -trimpath -o "$RUNNER_TEMP/tsgo" ./cmd/tsc';
const MAESTRO_COMMAND = "cargo test -p vize_maestro -- --quiet";
const MAESTRO_LOOP = "for iteration in $(seq 1 20); do";
const MAESTRO_SUCCESS_LOG = 'echo "Content Mapper Maestro lifecycle cycle $iteration/20 passed"';
const PACKAGE_ROUTE_COMMANDS = [
  "cargo test -p vize --test check_importer_scoped_package_cli -- --nocapture",
  "cargo test -p vize --test check_package_declaration_barrel_cli -- --nocapture",
  "cargo test -p vize --test check_package_paths_authority_cli -- --nocapture",
  "cargo test -p vize --test check_package_resolution_modes_cli -- --nocapture",
  "cargo test -p vize --test check_package_scope_mode_cli -- --nocapture",
  "cargo test -p vize --test check_package_tsx_shadow_cli -- --nocapture",
];
const REQUIRED_TRIGGER_PATHS = [
  ".github/workflows/content-mapper-conformance.yml",
  "Cargo.lock",
  "Cargo.toml",
  "crates/vize/Cargo.toml",
  "crates/vize/src/commands/content_mapper.rs",
  "crates/vize/src/commands/content_mapper/**",
  "crates/vize/src/commands/check/**",
  "crates/vize/tests/content_mapper_tsgo_cli.rs",
  "crates/vize/tests/content_mapper_tsgo_directives.rs",
  "crates/vize/tests/content_mapper_tsgo_build.rs",
  "crates/vize/tests/content_mapper_tsgo_watch.rs",
  "crates/vize/tests/content_mapper_importer_scoped_packages.rs",
  "crates/vize/tests/check_*package*.rs",
  "crates/vize/tests/check_*package*/**",
  "crates/vize/tests/davinci_ts40_projection_cli.rs",
  "crates/vize/tests/snapshots/davinci_ts40_projection_cli__*.snap",
  "crates/vize/tests/content_mapper_tsgo_lsp.rs",
  "crates/vize/tests/content_mapper_tsgo_declaration_lsp.rs",
  "crates/vize/tests/content_mapper_tsgo_lsp_event_forms.rs",
  "crates/vize/tests/content_mapper_tsgo_lsp_event_forms/**",
  "crates/vize/tests/content_mapper_lsp_support/**",
  "crates/vize/tests/fixtures/content_mapper_project/**",
  "crates/vize_canon/Cargo.toml",
  "crates/vize_canon/src/batch.rs",
  "crates/vize_canon/src/batch/**",
  "crates/vize_canon/src/corsa_bridge.rs",
  "crates/vize_canon/src/corsa_bridge/**",
  "crates/vize_canon/src/corsa_server.rs",
  "crates/vize_canon/src/corsa_server/**",
  "crates/vize_canon/src/lib.rs",
  "crates/vize_canon/src/lsp_client.rs",
  "crates/vize_canon/src/lsp_client/**",
  "crates/vize_canon/src/package_route.rs",
  "crates/vize_canon/src/package_route/**",
  "crates/vize_canon/tests/lsp_import_resolution.rs",
  "crates/vize_canon/src/virtual_ts.rs",
  "crates/vize_canon/src/virtual_ts/**",
  "crates/vize_maestro/Cargo.toml",
  "crates/vize_maestro/src/lib.rs",
  "crates/vize_maestro/src/ide/**",
  "crates/vize_maestro/src/server/**",
  "crates/vize_maestro/src/virtual_code.rs",
  "crates/vize_maestro/src/virtual_code/**",
  "crates/vize_maestro/tests/davinci_ts40_projection.rs",
  "crates/vize_maestro/tests/davinci_ts40_projection_support/**",
  "crates/vize_maestro/tests/snapshots/davinci_ts40_projection__*.snap",
  "npm/cli/bin/vize",
  "npm/cli/package.json",
  "npm/cli/src/**",
  "npm/cli/tests/**",
  "npm/native/**",
  "pnpm-lock.yaml",
  "pnpm-workspace.yaml",
  "tools/commands/release/npm/**",
  "tools/support/**",
  "tests/_fixtures/davinci-ts40-projection/**",
  "tests/tooling/davinci-ts40-projection.test.ts",
  "tests/tooling/support/davinci-ts40-projection.ts",
];

interface WorkflowStep {
  env?: Record<string, string>;
  name?: string;
  run?: string;
  uses?: string;
  with?: Record<string, unknown>;
  "working-directory"?: string;
}

interface WorkflowTrigger {
  branches?: string[];
  paths?: string[];
}

interface WorkflowConfig {
  on: {
    pull_request: WorkflowTrigger;
    push: WorkflowTrigger;
    workflow_dispatch?: unknown;
  };
}

function jobSteps(workflow: string, jobName: string): WorkflowStep[] {
  const jobs = (parse(workflow) as { jobs: Record<string, { steps?: WorkflowStep[] }> }).jobs;
  const steps = jobs[jobName]?.steps;
  assert.ok(Array.isArray(steps), `missing steps for job ${jobName}`);
  return steps;
}

function stepsRunning(steps: WorkflowStep[], command: string): number[] {
  return steps.flatMap((step, index) => (step.run?.includes(command) ? [index] : []));
}

function assertTriggerFilters(workflowConfig: WorkflowConfig): void {
  assert.ok(Object.hasOwn(workflowConfig.on, "workflow_dispatch"), "missing workflow_dispatch");
  for (const event of ["pull_request", "push"] as const) {
    const trigger = workflowConfig.on[event];
    assert.deepEqual(trigger.branches, ["main"], `${event} trigger must target main`);
    assert.ok(Array.isArray(trigger.paths), `${event} trigger must declare paths`);
    for (const pathFilter of REQUIRED_TRIGGER_PATHS) {
      assert.ok(trigger.paths.includes(pathFilter), `${event} paths missing ${pathFilter}`);
    }
  }
}

test("Content Mapper conformance pins and runs the exact upstream project path", () => {
  const workflow = readRepoFile(".github", "workflows", "content-mapper-conformance.yml");
  const workflowConfig = parse(workflow) as WorkflowConfig;
  const job = workflowJobBody(workflow, "exact-tsgo-project");
  const steps = jobSteps(workflow, "exact-tsgo-project");

  assertTriggerFilters(workflowConfig);

  assert.match(workflow, new RegExp(`CONTENT_MAPPER_TYPESCRIPT_SHA: "${UPSTREAM_SHA}"`));
  assert.match(job, /repository: microsoft\/TypeScript/);
  assert.match(job, /ref: \$\{\{ env\.CONTENT_MAPPER_TYPESCRIPT_SHA \}\}/);
  const upstreamCheckoutStep = steps.find(
    (step) => step.name === "Checkout exact TypeScript Content Mapper revision",
  );
  assert.ok(upstreamCheckoutStep, "missing exact TypeScript Content Mapper checkout step");
  assert.equal(
    upstreamCheckoutStep.uses,
    "actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd",
  );
  assert.equal(upstreamCheckoutStep.with?.repository, "microsoft/TypeScript");
  assert.equal(upstreamCheckoutStep.with?.ref, "${{ env.CONTENT_MAPPER_TYPESCRIPT_SHA }}");
  assert.equal(upstreamCheckoutStep.with?.path, "typescript-content-mapper");
  assert.equal(upstreamCheckoutStep.with?.["persist-credentials"], false);
  assert.match(job, /uses: actions\/setup-go@[0-9a-f]{40}\s+# v6\.1\.0/);
  assert.match(
    job,
    /uses: \.\/\.github\/actions\/setup-rust-sticky-cache\n\s+with:\n\s+key: content-mapper-conformance\n\s+cache-key-suffix: \$\{\{ runner\.os \}\}-\$\{\{ runner\.arch \}\}/,
  );
  assert.match(job, /go-version-file: typescript-content-mapper\/tsc\/go\.mod/);
  const contentMapperProtocolSteps = stepsRunning(steps, CONTENT_MAPPER_PROTOCOL_COMMAND);
  const spanmapProtocolSteps = stepsRunning(steps, SPANMAP_PROTOCOL_COMMAND);
  const buildSteps = stepsRunning(steps, TSGO_BUILD_COMMAND);
  assert.equal(
    contentMapperProtocolSteps.length,
    1,
    "expected exactly one upstream content mapper protocol regression step",
  );
  assert.equal(
    spanmapProtocolSteps.length,
    1,
    "expected exactly one upstream spanmap regression step",
  );
  assert.equal(buildSteps.length, 1, "expected exactly one exact tsgo build step");
  assert.equal(
    steps[contentMapperProtocolSteps[0]]["working-directory"],
    "typescript-content-mapper/tsc",
  );
  assert.equal(
    steps[spanmapProtocolSteps[0]]["working-directory"],
    "typescript-content-mapper/tsc",
  );
  assert.ok(
    contentMapperProtocolSteps[0] < buildSteps[0] && spanmapProtocolSteps[0] < buildSteps[0],
    "upstream protocol regressions must run before the exact tsgo build",
  );
  assert.match(job, /cp internal\/bundled\/libs\/\*\.d\.ts "\$RUNNER_TEMP\/"/);
  assert.match(job, /VIZE_TEST_CONTENT_MAPPER_TSGO: \$\{\{ runner\.temp \}\}\/tsgo/);
  assert.match(
    job,
    /VIZE_TEST_CONTENT_MAPPER_JAVASCRIPT_TSC: \$\{\{ github\.workspace \}\}\/npm\/cli\/node_modules\/\.bin\/tsc/,
  );
  assert.match(job, /cargo test -p vize --test content_mapper_tsgo_cli -- --nocapture/);
  assert.match(job, /cargo test -p vize --test content_mapper_tsgo_directives -- --nocapture/);
  assert.match(job, /cargo test -p vize --test content_mapper_tsgo_build -- --nocapture/);
  assert.match(job, /cargo test -p vize --test content_mapper_tsgo_watch -- --nocapture/);
  assert.match(job, /vp run --filter '\.\/npm\/native' build:ci/);
  assert.match(job, /\(cd npm\/cli && vp pack\)/);
  assert.match(job, /vp exec napi create-npm-dirs/);
  assert.match(job, /cp "\$binary" "npm\/\$target\/"/);
  assert.match(
    job,
    /VIZE_TEST_CONTENT_MAPPER_TSGO: \$\{\{ runner\.temp \}\}\/tsgo[\s\S]*smoke-release-install\.rs --prepare-manifests --content-mapper-checks[\s\S]*npm\/native npm\/native\/npm\/\*[\s\S]*npm\/cli/,
  );
  const packageRouteSteps = steps.flatMap((step, index) =>
    PACKAGE_ROUTE_COMMANDS.every((command) => step.run?.includes(command)) ? [index] : [],
  );
  assert.equal(packageRouteSteps.length, 1, "expected one package resolution matrix step");
  assert.ok(buildSteps[0] < packageRouteSteps[0], "package resolution must use exact tsgo");
  const packageRouteStep = steps[packageRouteSteps[0]];
  assert.equal(packageRouteStep.name, "Run importer-scoped package resolution matrix");
  assert.equal(packageRouteStep.env?.CORSA_PATH, "${{ runner.temp }}/tsgo");
  assert.equal(packageRouteStep.env?.VIZE_TEST_REQUIRE_TSGO, "1");
  const editorCommands = [
    "cargo test -p vize --test content_mapper_tsgo_lsp -- --nocapture",
    "cargo test -p vize --test content_mapper_tsgo_declaration_lsp -- --nocapture",
    "cargo test -p vize --test content_mapper_tsgo_lsp_event_forms -- --nocapture",
    "cargo test -p vize_canon --test lsp_import_resolution -- --nocapture",
  ];
  const editorSteps = steps.filter((step) =>
    editorCommands.every((command) => step.run?.includes(command)),
  );
  assert.equal(editorSteps.length, 1, "expected one mixed editor backend step");
  assert.equal(editorSteps[0].name, "Run Content Mapper LSP and virtual-overlay editor checks");
  assert.equal(editorSteps[0].env?.TSGO_PATH, "${{ runner.temp }}/tsgo");
  assert.equal(editorSteps[0].env?.VIZE_TEST_CONTENT_MAPPER_TSGO, "${{ runner.temp }}/tsgo");
  const stressSteps = stepsRunning(steps, MAESTRO_COMMAND);
  assert.equal(stressSteps.length, 1, "expected exactly one Maestro lifecycle stress step");
  const stressStep = steps[stressSteps[0]];
  const stressRun = stressStep.run ?? "";
  assert.equal(stressStep.env?.TSGO_PATH, "${{ runner.temp }}/tsgo");
  assert.equal(stressStep.env?.VIZE_TEST_CONTENT_MAPPER_TSGO, "${{ runner.temp }}/tsgo");
  assert.equal(
    [...stressRun.matchAll(/^\s*cargo test -p vize_maestro -- --quiet\s*$/gm)].length,
    1,
    "expected one direct Maestro command in the iteration loop",
  );
  assert.equal(
    [...stressRun.matchAll(/^\s*(?:for|while|until)\b/gm)].length,
    1,
    "retry loops are forbidden in the lifecycle stress step",
  );
  assert.doesNotMatch(
    stressRun,
    /\bsleep\b|--test-threads(?:=|\s+)1|RUST_TEST_THREADS\s*=\s*1|\|\|\s*true|\bset\s+\+e\b|\b(?:retry|attempt)\b|(?:grep|rg).*\b(?:panic|fail)/i,
    "stress step must not serialize, retry, sleep, ignore failures, or filter panics",
  );
  const loopStart = stressRun.indexOf(MAESTRO_LOOP);
  assert.ok(loopStart >= 0, "Maestro stress step must run a 20-iteration loop");
  const maestroRun = stressRun.indexOf(MAESTRO_COMMAND);
  const successLog = stressRun.indexOf(MAESTRO_SUCCESS_LOG);
  const loopEnd = stressRun.slice(loopStart).search(/^\s*done\s*$/m) + loopStart;
  assert.ok(loopStart < maestroRun, "Maestro command must run inside the loop");
  assert.ok(maestroRun < successLog, "success log must follow the Maestro command");
  assert.ok(successLog < loopEnd, "success log must run inside the same loop");
});
