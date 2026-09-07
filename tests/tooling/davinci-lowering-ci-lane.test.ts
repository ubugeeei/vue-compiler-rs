import assert from "node:assert/strict";
import { test } from "node:test";

import { readRepoFile, workflowJobBody } from "./support/github-workflows.ts";

const s1ToS2LoweringCorpusCommand =
  "cargo test -p vize_s1_to_s2 --features davinci-differential --test davinci_lowering_corpus -- --nocapture";
const s1ToS2DomCorpusCommand =
  "cargo test -p vize_s1_to_s2 --features davinci-differential --test davinci_dom_corpus -- --nocapture";
const jsxS2VdomParityCommand =
  "cargo test -p vize_atelier_jsx --features davinci-differential --lib vdom::s2_differential::s2_vdom_admitted_cases_match_relief_codegen -- --exact";

test("feature-gated S1-to-S2 corpus lanes ride the required clippy-and-test job", () => {
  const workflow = readRepoFile(".github", "workflows", "check.yml");
  const clippyJob = workflowJobBody(workflow, "clippy-and-test");
  const testReportJob = workflowJobBody(workflow, "test-report");
  const manifest = readRepoFile("crates", "vize_s1_to_s2", "Cargo.toml");
  const suites = readRepoFile("davinci-road", "plan", "test-suites.md");

  assert.match(suites, /\| TS-20 \| Lowering totality fuzz\s+\| `cargo test -p vize_s1_to_s2`/);
  for (const name of ["davinci_lowering_corpus", "davinci_dom_corpus"]) {
    assert.match(
      manifest,
      new RegExp(
        `^\\[\\[test\\]\\]\\nname = "${name}"\\nrequired-features = \\["davinci-differential"\\]$`,
        "m",
      ),
    );
  }

  assert.doesNotMatch(
    clippyJob,
    /^ {4}if:/m,
    "clippy-and-test must stay unconditional so TS-20 runs on pull requests",
  );
  assert.match(testReportJob, /- clippy-and-test\b/);
  assert.match(clippyJob, /run: cargo test --workspace && /);
  assert.ok(
    clippyJob.includes(s1ToS2LoweringCorpusCommand),
    "the feature-gated lowering corpus entry must run explicitly after cargo test --workspace",
  );
  assert.ok(
    clippyJob.includes(s1ToS2DomCorpusCommand),
    "the feature-gated DOM corpus entry must run explicitly after cargo test --workspace",
  );
  assert.match(
    readRepoFile("crates", "vize_atelier_jsx", "Cargo.toml"),
    /^davinci-differential = \[\]$/m,
  );
  assert.ok(
    clippyJob.includes(jsxS2VdomParityCommand),
    "the feature-gated JSX S2 VDOM parity entry must run explicitly after cargo test --workspace",
  );
});
