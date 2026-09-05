#![allow(unused_crate_dependencies)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use snapbox::{assert_data_eq, str};
use std::{
    fs,
    io::{self, Read},
    path::Path,
    process::Command,
};

fn read_json(path: &Path) -> Value {
    serde_json::from_reader(fs::File::open(path).unwrap()).unwrap()
}

fn read_text(path: &Path) -> String {
    let mut text = String::new();
    fs::File::open(path).unwrap().read_to_string(&mut text).unwrap();
    text
}

fn assert_hex_digest(value: &Value, digits: usize) {
    let digest = value.as_str().unwrap();
    assert_eq!(digest.len(), digits, "{digest}");
    assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()), "{digest}");
}

#[test]
fn dispatcher_preserves_out_of_order_messages_and_server_requests() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external pure returns (uint) { return 1; } }\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("servers.lock.yaml"),
        format!(
            r#"version: 1
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_EXPECT_TOOLCHAIN: "1"
    configuration:
      solidity:
        compiler: fake
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
        ),
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
servers_lock: servers.lock.yaml
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 2000
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    solc:
      version: "1"
      native: "{}"
    anchors:
      call:
        path: Main.sol
        needle: call
      main:
        path: Main.sol
        needle: contract Main
        offset: 9
scenarios:
  - id: smoke
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: cold-ready
        probe:
          kind: hover
          path: Main.sol
          anchor: call
          expected_text: add
      - kind: warm
        warmup: 0
        samples: 1
        probe:
          kind: completion
          path: Main.sol
          anchor: call
          expected_label: add
  - id: cache-reuse
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: cache-populated
        probe:
          kind: hover
          path: Main.sol
          anchor: call
          expected_text: add
      - kind: restart
      - kind: open
        path: Main.sol
      - kind: probe
        name: cold-ready
        probe:
          kind: hover
          path: Main.sol
          anchor: call
          expected_text: cache-reused
  - id: edit-save
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: replace
        path: Main.sol
        anchor: main
        text: contract Renamed
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
      - kind: save
        path: Main.sol
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
  - id: symbol-rename
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: rename
        path: Main.sol
        anchor: main
        new_name: Renamed
        expected_edits:
          - path: Main.sol
            anchor: main
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
  - id: file-lifecycle
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: create-file
        path: Scratch.sol
        text: "pragma solidity ^0.8.30; contract Scratch {{}}"
        probe:
          kind: workspace-symbol
          query: Scratch
          expected_name: Scratch
          expected_path: Scratch.sol
      - kind: open
        path: Scratch.sol
      - kind: rename-file
        from: Scratch.sol
        to: Renamed.sol
        probe:
          kind: workspace-symbol
          query: Scratch
          expected_name: Scratch
          expected_path: Renamed.sol
      - kind: save
        path: Renamed.sol
        probe:
          kind: document-symbol
          path: Renamed.sol
          expected_name: Scratch
      - kind: delete-file
        path: Renamed.sol
        probe:
          kind: workspace-symbol
          query: Scratch
          expected_name: Scratch
          expected_path: Renamed.sol
          present: false
  - id: cache-recovery
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: cache-populated
        probe:
          kind: hover
          path: Main.sol
          anchor: call
          expected_text: add
      - kind: restart
        invalidate:
          path: Main.sol
          anchor: main
          text: contract Recovered
      - kind: open
        path: Main.sol
      - kind: probe
        name: cold-ready
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Recovered
"#,
            fixture.display(),
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--output"])
        .arg(&output)
        // The fake server fails if the harness forwards this ambient variable.
        .env("LSP_BENCH_AMBIENT_SECRET_CANARY", "must-not-reach-server")
        .status()
        .unwrap();
    if !status.success() {
        eprintln!("benchmark failed; output at {}", output.display());
    }

    let summary = read_json(&output.join("summary.json"));
    for group in summary["summaries"].as_array().unwrap() {
        if group["status_counts"]["pass"] != 1 {
            eprintln!("failed group: {group}");
        }
    }
    let samples = read_json(&output.join("samples.json"));
    for sample in samples["samples"].as_array().unwrap() {
        if sample["status"] != "pass" {
            eprintln!("failed sample: {sample}");
        }
    }
    assert!(
        summary["summaries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|group| group["status_counts"]["pass"] == 1)
    );
    assert_eq!(summary["environment"]["network_isolated"], false);
    assert_hex_digest(&summary["harness_git_revision"], 40);
    assert!(summary["harness_git_dirty"].is_boolean());
    assert_hex_digest(&summary["harness_contract_sha256"], 64);
    assert_hex_digest(&summary["harness_executable_sha256"], 64);
    assert_eq!(
        summary["harness_executable_sha256"].as_str().unwrap(),
        sha256_path(Path::new(env!("CARGO_BIN_EXE_solar-lsp-bench")))
    );
    assert_hex_digest(&summary["servers"][0]["executable_sha256"], 64);
    assert_hex_digest(&summary["fixtures"][0]["content_sha256"], 64);
    assert_hex_digest(&summary["fixtures"][0]["solc_native_sha256"], 64);
    let smoke = samples["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["workload"] == "smoke")
        .unwrap();
    assert_eq!(smoke["observations"]["diagnostic_publications"], 1);
    assert!(smoke["timings_ms"]["ready_ms"].is_number());
    assert!(
        smoke["correctness"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| { result["probe"] == "ready" && result["ok"] == true })
    );
    let measured_requests = smoke["observations"]["requests"].as_array().unwrap();
    assert_eq!(measured_requests.len(), 1);
    assert_eq!(measured_requests[0]["method"], "textDocument/completion");
    let server_requests = smoke["observations"]["server_requests"].as_array().unwrap();
    for method in [
        "window/workDoneProgress/create",
        "workspace/configuration",
        "client/registerCapability",
        "workspace/applyEdit",
    ] {
        assert!(
            server_requests
                .iter()
                .any(|request| request["method"] == method && request["handled"] == true),
            "server request {method} was not handled"
        );
    }
    assert!(smoke["observations"]["events"].as_array().unwrap().iter().any(|event| {
        event["direction"] == "receive" && event["id"] == 999 && event["method"].is_null()
    }));
    let cache = samples["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["workload"] == "cache-reuse")
        .unwrap();
    assert_eq!(cache["setup_phases"].as_array().unwrap().len(), 1);
    assert_eq!(cache["status"], "pass");
    assert!(cache["timings_ms"]["cold_ready_ms"].as_f64().unwrap() >= 70.0);
    let edit = samples["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["workload"] == "edit-save")
        .unwrap();
    assert!(edit["timings_ms"]["edit_to_edit-ready_ms"].is_number());
    assert!(edit["timings_ms"]["save_to_save-ready_ms"].is_number());
    for method in ["textDocument/didChange", "textDocument/didSave"] {
        assert!(
            edit["observations"]["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["direction"] == "send" && event["method"] == method),
            "missing {method}"
        );
    }
    let rename = samples["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["workload"] == "symbol-rename")
        .unwrap();
    assert_eq!(rename["status"], "pass");
    assert!(rename["timings_ms"]["rename_to_rename-ready_ms"].is_number());
    let lifecycle = samples["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["workload"] == "file-lifecycle")
        .unwrap();
    assert_eq!(lifecycle["status"], "pass");
    for timing in [
        "create-file_to_create-file-ready_ms",
        "rename-file_to_rename-file-ready_ms",
        "delete-file_to_delete-file-ready_ms",
    ] {
        assert!(lifecycle["timings_ms"][timing].is_number(), "missing {timing}");
    }
    let sent_methods = lifecycle["observations"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["direction"] == "send")
        .filter_map(|event| event["method"].as_str())
        .collect::<Vec<_>>();
    let events = lifecycle["observations"]["events"].as_array().unwrap();
    let opened_uris = events
        .iter()
        .filter(|event| event["direction"] == "send" && event["method"] == "textDocument/didOpen")
        .filter_map(|event| {
            event.pointer("/message/params/textDocument/uri").and_then(Value::as_str)
        })
        .collect::<Vec<_>>();
    assert_eq!(opened_uris.len(), 3, "lifecycle probes must not open their targets");
    assert!(opened_uris[0].ends_with("/Main.sol"));
    assert!(opened_uris[1].ends_with("/Scratch.sol"));
    assert!(opened_uris[2].ends_with("/Renamed.sol"));
    let closed_uris = events
        .iter()
        .filter(|event| event["direction"] == "send" && event["method"] == "textDocument/didClose")
        .filter_map(|event| {
            event.pointer("/message/params/textDocument/uri").and_then(Value::as_str)
        })
        .collect::<Vec<_>>();
    assert_eq!(closed_uris.len(), 2);
    assert!(closed_uris[0].ends_with("/Scratch.sol"));
    assert!(closed_uris[1].ends_with("/Renamed.sol"));
    for lifecycle in [
        ["workspace/willCreateFiles", "workspace/didCreateFiles"],
        ["workspace/willRenameFiles", "workspace/didRenameFiles"],
        ["workspace/willDeleteFiles", "workspace/didDeleteFiles"],
    ] {
        let positions = lifecycle.map(|method| {
            sent_methods
                .iter()
                .position(|sent| sent == &method)
                .unwrap_or_else(|| panic!("missing {method}"))
        });
        assert!(positions[0] < positions[1], "{} must precede {}", lifecycle[0], lifecycle[1]);
    }
    let will_create =
        sent_methods.iter().position(|method| method == &"workspace/willCreateFiles").unwrap();
    let apply_preflight_edit =
        sent_methods.iter().position(|method| method == &"textDocument/didChange").unwrap();
    let did_create =
        sent_methods.iter().position(|method| method == &"workspace/didCreateFiles").unwrap();
    assert!(will_create < apply_preflight_edit && apply_preflight_edit < did_create);
    let event_position = |method: &str, suffix: &str| {
        events
            .iter()
            .position(|event| {
                event["direction"] == "send"
                    && event["method"] == method
                    && event
                        .pointer("/message/params/textDocument/uri")
                        .and_then(Value::as_str)
                        .is_some_and(|uri| uri.ends_with(suffix))
            })
            .unwrap_or_else(|| panic!("missing {method} for {suffix}"))
    };
    let sent_event_position = |method: &str| {
        events
            .iter()
            .position(|event| event["direction"] == "send" && event["method"] == method)
            .unwrap_or_else(|| panic!("missing {method}"))
    };
    let close_old = event_position("textDocument/didClose", "/Scratch.sol");
    let open_new = event_position("textDocument/didOpen", "/Renamed.sol");
    let will_rename = sent_event_position("workspace/willRenameFiles");
    let did_rename = sent_event_position("workspace/didRenameFiles");
    assert!(will_rename < close_old && close_old < open_new && open_new < did_rename);
    let save_new = event_position("textDocument/didSave", "/Renamed.sol");
    let close_deleted = event_position("textDocument/didClose", "/Renamed.sol");
    let will_delete = sent_event_position("workspace/willDeleteFiles");
    let did_delete = sent_event_position("workspace/didDeleteFiles");
    assert!(open_new < save_new && save_new < will_delete);
    assert!(will_delete < close_deleted && close_deleted < did_delete);
    let recovery = samples["samples"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["workload"] == "cache-recovery")
        .unwrap();
    assert_eq!(recovery["setup_phases"].as_array().unwrap().len(), 1);
    assert_eq!(recovery["status"], "pass");
    assert!(recovery["timings_ms"]["cold_ready_ms"].is_number());
    assert!(
        samples["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|sample| sample["process"]["network_isolated"] == false)
    );

    let summary_markdown = read_text(&output.join("summary.md"));
    assert_data_eq!(
        &summary_markdown,
        str![[r#"
# Cross-server Solidity LSP benchmark
...
## Run metadata
...
## Servers
...
## Results

| Server | Fixture | Workload | Capabilities | Successful | Statuses | Result | Metric | p50 | p95 | p99 | Max |
...
"#]],
    );
    let sample_count = samples["samples"].as_array().unwrap().len();
    let jsonl = read_text(&output.join("samples.jsonl"));
    assert_eq!(jsonl.lines().count(), sample_count);

    let regenerated_markdown = directory.path().join("regenerated.md");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["report", "--input"])
        .arg(output.join("summary.json"))
        .args(["--output"])
        .arg(&regenerated_markdown)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(summary_markdown, read_text(&regenerated_markdown));
}

fn sha256_path(path: &Path) -> String {
    let mut file = fs::File::open(path).unwrap();
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).unwrap();
    format!("{:x}", hasher.finalize())
}

#[test]
fn file_lifecycle_rejects_a_server_that_ignores_notifications() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external pure returns (uint) { return 1; } }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 300
servers:
  - id: ignores-files
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: ignore-file-notifications
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: create
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: create-file
        path: Scratch.sol
        text: "contract Scratch {{}}"
        probe:
          kind: workspace-symbol
          query: Scratch
          expected_name: Scratch
          expected_path: Scratch.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    assert_eq!(samples["samples"][0]["status"], "incorrect");
    assert!(
        samples["samples"][0]["error"]
            .as_str()
            .unwrap()
            .contains("workspace symbols did not contain `Scratch`")
    );
}

#[test]
fn response_without_result_or_error_is_a_harness_error() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "contract Main { function value() external pure returns (uint) { return 1; } }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 300
servers:
  - id: missing-result
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: missing-negative-workspace-symbol-result
fixtures:
  - id: synthetic
    root: "{}"
scenarios:
  - id: lifecycle
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: create-file
        path: Scratch.sol
        text: "contract Scratch {{}}"
        probe:
          kind: workspace-symbol
          query: Scratch
          expected_name: Scratch
          expected_path: Scratch.sol
      - kind: delete-file
        path: Scratch.sol
        probe:
          kind: workspace-symbol
          query: Scratch
          expected_name: Scratch
          expected_path: Scratch.sol
          present: false
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    assert_eq!(samples["samples"][0]["status"], "harness-error");
    assert!(
        samples["samples"][0]["error"]
            .as_str()
            .unwrap()
            .contains("must contain exactly one of `result` or `error`")
    );
}

#[test]
fn probe_failures_distinguish_incorrect_results_from_timeouts() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external {} }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  failure:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 500
    readiness_quiet_ms: 20
servers:
  - id: incorrect
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: incorrect-hover
  - id: timeout
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: timeout-hover
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    solc:
      version: "1"
      native: "{}"
    anchors:
      call:
        path: Main.sol
        needle: call
scenarios:
  - id: failing-hover
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: cold-ready
        probe:
          kind: hover
          path: Main.sol
          anchor: call
          expected_text: add
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "failure", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let samples = samples["samples"].as_array().unwrap();
    let incorrect = samples.iter().find(|sample| sample["server"] == "incorrect").unwrap();
    assert_eq!(incorrect["status"], "incorrect");
    assert_eq!(incorrect["correctness"][0]["ok"], false);
    assert!(incorrect["error"].as_str().unwrap().contains("hover did not contain"));

    let timeout = samples.iter().find(|sample| sample["server"] == "timeout").unwrap();
    assert_eq!(timeout["status"], "timeout");
    assert_eq!(timeout["correctness"][0]["ok"], false);
    assert!(timeout["error"].as_str().unwrap().contains("timed out waiting for LSP message"));
}

#[test]
fn initialize_timeout_remains_a_timeout_after_process_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 200
servers:
  - id: timeout
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: timeout-initialize
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: initialize
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "timeout");
    assert!(sample["error"].as_str().unwrap().contains("timed out waiting for LSP message"));
}

#[cfg(unix)]
#[test]
fn descendant_cleanup_does_not_fail_a_successful_run() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
servers:
  - id: cleanup
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: leave-descendant-on-exit
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: restart
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: restart
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "pass");
    assert!(sample["error"].is_null());
    assert_eq!(sample["setup_phases"][0]["name"], "cache-population");
    for process in [&sample["setup_phases"][0]["process"], &sample["process"]] {
        assert_eq!(process["exit_code"], 0);
        assert_eq!(process["forced_kill"], true);
    }

    let summary = read_json(&output.join("summary.json"));
    assert_eq!(summary["environment"]["authoritative"], false);
    let group = &summary["summaries"][0];
    assert_eq!(group["status"], "pass");
    assert_eq!(group["successful_runs"], 1);
    assert_eq!(group["status_counts"]["pass"], 1);
    assert_eq!(group["metrics"]["cache_population_process_ms"]["count"], 1);
    assert_eq!(group["metrics"]["session_wall_ms"]["count"], 1);
}

#[test]
fn server_that_never_reads_stdin_is_bounded_by_request_timeout() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let initialization_payload = serde_json::to_string(&"x".repeat(1024 * 1024)).unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 200
servers:
  - id: blocked-stdin
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: never-read-stdin
    initialization_options:
      payload: {initialization_payload}
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: initialize
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let started = std::time::Instant::now();
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();

    assert!(status.success());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "timeout");
    assert!(sample["error"].as_str().unwrap().contains("timed out writing LSP message"));
}

#[test]
fn shutdown_crash_overrides_an_unsupported_workload() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 500
servers:
  - id: crash
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: no-text-sync-shutdown-crash
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      main:
        path: Main.sol
        needle: contract Main
        offset: 9
scenarios:
  - id: edit
    fixture: synthetic
    steps:
      - kind: replace
        path: Main.sol
        anchor: main
        text: contract Renamed
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "crash");
    assert_eq!(sample["process"]["exit_code"], 1);
    let error = sample["error"].as_str().unwrap();
    assert!(error.contains("does not advertise `textDocument/didChange`"), "{error}");
    assert!(error.contains("server exited with Some(1)"), "{error}");
}

#[test]
fn client_honors_negotiated_sync_save_and_completion_capabilities() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external {} }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
servers:
  - id: negotiated
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: negotiated-capabilities
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      call:
        path: Main.sol
        needle: call
      main:
        path: Main.sol
        needle: contract Main
        offset: 9
scenarios:
  - id: completion-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: completion
        probe:
          kind: completion
          path: Main.sol
          anchor: call
          expected_label: add
  - id: save-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: replace
        path: Main.sol
        anchor: main
        text: contract Renamed
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
      - kind: save
        path: Main.sol
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let samples = samples["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 2);
    assert!(samples.iter().all(|sample| sample["status"] == "pass"), "{samples:#?}");
    for sample in samples {
        assert!(!sample["observations"]["events"].as_array().unwrap().iter().any(|event| {
            event["direction"] == "send" && event["method"] == "textDocument/didOpen"
        }));
    }

    let completion =
        samples.iter().find(|sample| sample["workload"] == "completion-contract").unwrap();
    let request = completion["observations"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["direction"] == "send" && event["method"] == "textDocument/completion")
        .unwrap();
    assert_eq!(request.pointer("/message/params/context/triggerKind"), Some(&Value::from(1)));
    assert!(request.pointer("/message/params/context/triggerCharacter").is_none());

    let save = samples.iter().find(|sample| sample["workload"] == "save-contract").unwrap();
    let notification = save["observations"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["direction"] == "send" && event["method"] == "textDocument/didSave")
        .unwrap();
    assert!(notification.pointer("/message/params/text").is_none());
}

#[test]
fn probes_resolve_anchors_after_unsaved_multiline_unicode_edits() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main { function target() external {} }\n")
        .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: position-sensitive-hover
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      header:
        path: Main.sol
        needle: "contract Main {{"
      target:
        path: Main.sol
        needle: target
scenarios:
  - id: edited-anchor
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: replace
        path: Main.sol
        anchor: header
        text: "contract Main {{\n    string constant FACE = unicode\"😀\";"
        probe:
          kind: hover
          path: Main.sol
          anchor: target
          expected_text: function add
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"].as_array().unwrap()[0];
    assert_eq!(sample["status"], "pass", "{sample:#}");
}

#[test]
fn client_sends_open_for_numeric_text_sync_capability() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external {} }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
servers:
  - id: numeric
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: numeric-text-sync
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      call:
        path: Main.sol
        needle: call
scenarios:
  - id: hover-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: hover
        probe:
          kind: hover
          path: Main.sol
          anchor: call
          expected_text: add
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"].as_array().unwrap()[0];
    assert_eq!(sample["status"], "pass");
    let events = sample["observations"]["events"].as_array().unwrap();
    let initialize = events
        .iter()
        .find(|event| {
            event["direction"] == "receive"
                && event.pointer("/message/result/capabilities/textDocumentSync")
                    == Some(&Value::from(1))
        })
        .expect("numeric text sync capability response");
    assert_eq!(initialize["message"]["result"]["capabilities"]["textDocumentSync"], 1);
    assert!(events.iter().any(|event| {
        event["direction"] == "send" && event["method"] == "textDocument/didOpen"
    }));
}

#[test]
fn missing_text_sync_disables_notifications_and_edit_workloads() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external {} }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
servers:
  - id: no-sync
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: no-text-sync
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      call:
        path: Main.sol
        needle: call
      main:
        path: Main.sol
        needle: contract Main
        offset: 9
scenarios:
  - id: open-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: completion
        probe:
          kind: completion
          path: Main.sol
          anchor: call
          expected_label: add
  - id: change-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: replace
        path: Main.sol
        anchor: main
        text: contract Renamed
        probe:
          kind: completion
          path: Main.sol
          anchor: call
          expected_label: add
  - id: save-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: save
        path: Main.sol
        probe:
          kind: completion
          path: Main.sol
          anchor: call
          expected_label: add
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let samples = samples["samples"].as_array().unwrap();
    let status = |workload: &str| {
        &samples.iter().find(|sample| sample["workload"] == workload).unwrap()["status"]
    };
    assert_eq!(status("open-contract"), "pass");
    assert_eq!(status("change-contract"), "unsupported");
    assert_eq!(status("save-contract"), "unsupported");
    assert!(samples.iter().all(|sample| {
        !sample["observations"]["events"].as_array().unwrap().iter().any(|event| {
            event["direction"] == "send"
                && matches!(
                    event["method"].as_str(),
                    Some(
                        "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didSave"
                    )
                )
        })
    }));
}

#[test]
fn dynamic_text_sync_registration_controls_change_shape_and_save_text() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external {} }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
servers:
  - id: dynamic-sync
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: dynamic-text-sync
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      main:
        path: Main.sol
        needle: contract Main
        offset: 9
scenarios:
  - id: dynamic-sync-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: replace
        path: Main.sol
        anchor: main
        text: contract Renamed
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
      - kind: save
        path: Main.sol
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "pass", "{sample:#}");
    let events = sample["observations"]["events"].as_array().unwrap();
    let change = events
        .iter()
        .find(|event| event["direction"] == "send" && event["method"] == "textDocument/didChange")
        .unwrap();
    assert!(change.pointer("/message/params/contentChanges/0/range").is_some());
    let save = events
        .iter()
        .find(|event| event["direction"] == "send" && event["method"] == "textDocument/didSave")
        .unwrap();
    assert!(save.pointer("/message/params/text").is_some());
}

#[test]
fn dynamic_text_sync_registration_selector_gates_solidity_notifications() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "pragma solidity ^0.8.30; contract Main { function call() external {} }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
servers:
  - id: dynamic-sync
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: dynamic-text-sync-selector-mismatch
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      main:
        path: Main.sol
        needle: contract Main
        offset: 9
scenarios:
  - id: dynamic-sync-contract
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: replace
        path: Main.sol
        anchor: main
        text: contract Renamed
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Renamed
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "unsupported", "{sample:#}");
    assert!(sample["observations"]["events"].as_array().unwrap().iter().all(|event| {
        !(event["direction"] == "send"
            && matches!(
                event["method"].as_str(),
                Some("textDocument/didOpen")
                    | Some("textDocument/didChange")
                    | Some("textDocument/didSave")
            ))
    }));
}

#[test]
fn executable_with_a_mismatched_version_is_incompatible() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 500
servers:
  - id: incompatible
    command: "{}"
    version_args: [--version]
    locked_version: "2"
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: open
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let summary = read_json(&output.join("summary.json"));
    assert_eq!(summary["servers"][0]["status"], "incompatible");
    assert_eq!(summary["summaries"][0]["status"], "failed");
    let samples = read_json(&output.join("samples.json"));
    assert_eq!(samples["samples"][0]["status"], "incompatible");
}

#[test]
fn authoritative_profile_rejects_portable_results_after_writing_reports() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  publish:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 500
    require_authoritative: true
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: open
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "publish", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(!status.success());

    let summary = read_json(&output.join("summary.json"));
    assert_eq!(summary["environment"]["network_isolated"], false);
    assert_eq!(summary["environment"]["authoritative"], false);
    assert_eq!(summary["summaries"][0]["status_counts"]["pass"], 1);
    for artifact in ["summary.json", "samples.json", "samples.jsonl", "summary.md"] {
        assert!(output.join(artifact).is_file(), "missing run artifact {artifact}");
    }

    let regenerated = directory.path().join("regenerated.md");
    let report_status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["report", "--input"])
        .arg(output.join("summary.json"))
        .args(["--output"])
        .arg(&regenerated)
        .arg("--require-authoritative")
        .status()
        .unwrap();
    assert!(!report_status.success());
    assert!(!regenerated.exists());
}

#[test]
fn cold_readiness_rejects_invalid_results_for_unopened_sources() {
    for behavior in
        ["empty-unopened-symbols", "null-unopened-symbols", "malformed-unopened-symbols"]
    {
        let directory = tempfile::tempdir().unwrap();
        let fixture = directory.path().join("fixture");
        fs::create_dir(&fixture).unwrap();
        fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
        fs::write(fixture.join("Secondary.sol"), "contract Secondary {}\n").unwrap();
        let config = directory.path().join("benchmark.yaml");
        fs::write(
            &config,
            format!(
                r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 2000
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: {behavior}
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: cold
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: cold-ready
        probe:
          kind: document-symbol
          path: Main.sol
          expected_name: Main
"#,
                env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
                fixture.display(),
            ),
        )
        .unwrap();
        let output = directory.path().join("results");
        let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
            .args(["run", "--config"])
            .arg(&config)
            .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
            .arg(&output)
            .status()
            .unwrap();
        assert!(status.success());

        let samples = read_json(&output.join("samples.json"));
        assert_eq!(samples["samples"][0]["status"], "incorrect", "{behavior}");
        let error = samples["samples"][0]["error"].as_str().unwrap();
        if behavior == "malformed-unopened-symbols" {
            assert!(error.contains("document symbols returned an invalid result"), "{error}");
        } else {
            assert!(error.contains("document symbols returned 0 items"), "{error}");
        }
    }
}

#[test]
fn rename_rejects_workspace_edit_missing_an_expected_cross_file_change() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(
        fixture.join("Main.sol"),
        "contract Main { function call() external pure returns (uint) { return Math.double(1); } }\n",
    )
    .unwrap();
    fs::write(
        fixture.join("Math.sol"),
        "library Math { function double(uint value) internal pure returns (uint) { return value * 2; } }\n",
    )
    .unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 2000
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      call-double:
        path: Main.sol
        needle: Math.double
        offset: 5
      math-double:
        path: Math.sol
        needle: function double
        offset: 9
scenarios:
  - id: rename
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: rename
        path: Main.sol
        anchor: call-double
        new_name: doubled
        expected_edits:
          - path: Main.sol
            anchor: call-double
          - path: Math.sol
            anchor: math-double
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    assert_eq!(samples["samples"][0]["status"], "incorrect");
    let error = samples["samples"][0]["error"].as_str().unwrap();
    assert!(error.contains("rename WorkspaceEdit did not change `doubled`"), "{error}");
    assert!(error.contains("Math.sol"), "{error}");
}

#[test]
fn rename_rejects_an_overwide_expected_range_before_applying_it() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 2000
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: oversized-rename
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
    anchors:
      main:
        path: Main.sol
        needle: contract Main
        offset: 9
scenarios:
  - id: rename
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
      - kind: rename
        path: Main.sol
        anchor: main
        new_name: Renamed
        expected_edits:
          - path: Main.sol
            anchor: main
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "incorrect");
    let error = sample["error"].as_str().unwrap();
    assert!(error.contains("rename WorkspaceEdit did not change `Renamed`"), "{error}");
    assert!(error.contains("Main.sol"), "{error}");
    assert!(!sample["observations"]["events"].as_array().unwrap().iter().any(|event| {
        event["direction"] == "send" && event["method"] == "textDocument/didChange"
    }));
}

#[test]
fn stale_versioned_workspace_edit_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 2000
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: stale-versioned-edit
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: open
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "harness-error");
    let error = sample["error"].as_str().unwrap();
    assert!(error.contains("WorkspaceEdit targets version 0"), "{error}");
    assert!(error.contains("open document is version 1"), "{error}");
}

#[test]
fn incremental_workspace_apply_edit_uses_sequential_change_ranges() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    let original = "contract Main {}\n";
    fs::write(fixture.join("Main.sol"), original).unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 2000
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: multi-edit-apply
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: open
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "pass", "{sample:#}");
    let events = sample["observations"]["events"].as_array().unwrap();
    let request = events
        .iter()
        .position(|event| {
            event["direction"] == "receive"
                && event["method"] == "workspace/applyEdit"
                && event["id"] == "multi-edit"
        })
        .unwrap();
    let change = events
        .iter()
        .position(|event| {
            event["direction"] == "send" && event["method"] == "textDocument/didChange"
        })
        .unwrap();
    let response = events
        .iter()
        .position(|event| {
            event["direction"] == "send"
                && event["id"] == "multi-edit"
                && event["message"]["result"]["applied"] == true
        })
        .unwrap();
    assert!(request < change && change < response);

    let content_changes = events[change]["message"]["params"]["contentChanges"].as_array().unwrap();
    assert_eq!(content_changes.len(), 2);
    let mut synchronized = original.to_owned();
    for change in content_changes {
        let start = change["range"]["start"]["character"].as_u64().unwrap() as usize;
        let end = change["range"]["end"]["character"].as_u64().unwrap() as usize;
        synchronized.replace_range(start..end, change["text"].as_str().unwrap());
    }
    assert_eq!(synchronized, "abstract contract Renamed {}\n");
}

#[test]
fn shutdown_applies_queued_workspace_edits_before_acknowledging_them() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = directory.path().join("fixture");
    fs::create_dir(&fixture).unwrap();
    fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
    let config = directory.path().join("benchmark.yaml");
    fs::write(
        &config,
        format!(
            r#"version: 1
profiles:
  smoke:
    warmup: 0
    samples: 1
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 2000
servers:
  - id: fake
    command: "{}"
    version_args: [--version]
    env:
      LSP_BENCH_FAKE_BEHAVIOR: shutdown-apply-edit
fixtures:
  - id: synthetic
    root: "{}"
    source_roots: [.]
scenarios:
  - id: open
    fixture: synthetic
    steps:
      - kind: open
        path: Main.sol
"#,
            env!("CARGO_BIN_EXE_solar-lsp-bench-fake"),
            fixture.display(),
        ),
    )
    .unwrap();
    let output = directory.path().join("results");
    let status = Command::new(env!("CARGO_BIN_EXE_solar-lsp-bench"))
        .args(["run", "--config"])
        .arg(&config)
        .args(["--profile", "smoke", "--repeat", "1", "--allow-failures", "--output"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());

    let samples = read_json(&output.join("samples.json"));
    let sample = &samples["samples"][0];
    assert_eq!(sample["status"], "pass");
    let events = sample["observations"]["events"].as_array().unwrap();
    let request = events
        .iter()
        .position(|event| {
            event["direction"] == "receive"
                && event["method"] == "workspace/applyEdit"
                && event["id"] == "shutdown-edit"
        })
        .unwrap();
    let change = events
        .iter()
        .position(|event| {
            event["direction"] == "send" && event["method"] == "textDocument/didChange"
        })
        .unwrap();
    let response = events
        .iter()
        .position(|event| {
            event["direction"] == "send"
                && event["id"] == "shutdown-edit"
                && event["message"]["result"]["applied"] == true
        })
        .unwrap();
    assert!(request < change && change < response);
}
