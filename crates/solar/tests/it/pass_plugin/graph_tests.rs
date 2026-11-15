use super::*;

#[test]
fn graph_examples_skip_gas_observers() {
    let directory = tempfile::tempdir().unwrap();
    for (extension, source, pipeline, baseline_pipeline) in [
        (
            "mir",
            MIR.replace("calldataload 0", "gas"),
            "-Zmir-pipeline=example::create-lowered-helper",
            "-Zmir-pipeline=none",
        ),
        (
            "mir",
            MIR.replace("calldataload 0", "gas"),
            "-Zmir-pipeline=example::split-lowered-block",
            "-Zmir-pipeline=none",
        ),
        (
            "evmir",
            EVM.replace("calldatasize", "gas"),
            "-Zevm-ir-pipeline=example::create-evm-helper",
            "-Zevm-ir-pipeline=none",
        ),
    ] {
        let path = directory.path().join(format!("test.{extension}"));
        fs::write(&path, source).unwrap();
        let output = run(&path, &sample(), &["-Ogas", pipeline, "-Zprint-after-each"]);
        let baseline = run(&path, &sample(), &["-Ogas", baseline_pipeline, "-Zprint-after-each"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(baseline.status.success(), "{}", String::from_utf8_lossy(&baseline.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        let baseline = String::from_utf8(baseline.stdout).unwrap();
        assert_eq!(
            &text[text.find("@module").unwrap()..],
            &baseline[baseline.find("@module").unwrap()..]
        );
    }
}

#[test]
fn invalid_graphs_are_rejected_without_optional_validation() {
    let directory = tempfile::tempdir().unwrap();
    let mut diagnostics = Vec::new();
    for (extension, source, modes, pipeline) in [
        (
            "mir",
            "@module BadGraph\nfn @f(arg0: i256) -> i256 {\nbb0:\n ret arg0\n}\n",
            &[
                "graph_bad_type",
                "graph_phi",
                "graph_dominance",
                "graph_bad_signature",
                "graph_duplicate_local",
                "graph_bad_constant",
                "graph_duplicate_name",
                "graph_split_bounds",
                "graph_void_phi",
            ][..],
            "-Zmir-pipeline=broken::fail",
        ),
        ("mir", MIR, &["graph_bad_phase", "graph_copy_result"][..], "-Zmir-pipeline=broken::fail"),
        (
            "evmir",
            EVM,
            &["graph_bad_opcode", "graph_bad_call_effect", "graph_bad_target"][..],
            "-Zevm-ir-pipeline=broken::fail",
        ),
    ] {
        let path = directory.path().join(format!("test.{extension}"));
        fs::write(&path, source).unwrap();
        for &mode in modes {
            let library = broken(directory.path(), mode);
            let output =
                run(&path, &library, &["-Ogas", pipeline, "-Zvalidate-ir=false", "--emit=bin"]);
            assert!(!output.status.success(), "{mode} unexpectedly succeeded");
            assert!(output.stdout.is_empty(), "invalid graph emitted an artifact");
            diagnostics.push((mode, diagnostic_messages(&output)));
        }
    }
    assert_data_eq!(
        serde_json::to_string_pretty(&diagnostics).unwrap(),
        str![[r#"
[
  [
    "graph_bad_type",
    [
      "plugin pass `broken::fail` failed: unsupported MIR integer width",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_phi",
    [
      "[fn1] [bb1, inst0] phi missing incoming entry for predecessor bb0",
      "plugin pass `broken::fail` failed: produced invalid MIR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "graph_dominance",
    [
      "[fn0] [bb0, inst0] use of ValueId(1) precedes its definition in this block",
      "plugin pass `broken::fail` failed: produced invalid MIR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "graph_bad_signature",
    [
      "[fn1] icall to `f` passes 0 argument(s), expected 1",
      "plugin pass `broken::fail` failed: produced invalid MIR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "graph_duplicate_local",
    [
      "plugin pass `broken::fail` failed: duplicate response-local SSA definition",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_bad_constant",
    [
      "plugin pass `broken::fail` failed: constant does not fit its scalar MIR type",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_duplicate_name",
    [
      "plugin pass `broken::fail` failed: invalid or duplicate new MIR function name",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_split_bounds",
    [
      "plugin pass `broken::fail` failed: MIR split position is out of bounds",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_void_phi",
    [
      "plugin pass `broken::fail` failed: new MIR phi requires an SSA result",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_bad_phase",
    [
      "[fn1] non-word type `slice<memory>` survives the `lowered` phase boundary",
      "plugin pass `broken::fail` failed: produced invalid MIR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "graph_copy_result",
    [
      "plugin pass `broken::fail` failed: copied MIR result representation differs",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_bad_opcode",
    [
      "EVM IR verification failed: block 1: terminator opcode `0x01` is not terminal",
      "plugin pass `broken::fail` failed: produced invalid EVM IR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "graph_bad_call_effect",
    [
      "plugin pass `broken::fail` failed: call effect does not consume its return address",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "graph_bad_target",
    [
      "plugin pass `broken::fail` failed: unknown EVM IR block",
      "aborting due to 1 previous error"
    ]
  ]
]
"#]]
    );
}

#[test]
fn plugins_create_mir_functions_blocks_and_loop_phis() {
    let directory = tempfile::tempdir().unwrap();
    let sample = sample();
    let mut snapshots = Vec::new();
    for (phase, pass) in [("semantic", "create-mir-helper"), ("lowered", "create-lowered-helper")] {
        let path = directory.path().join("test.mir");
        fs::write(&path, MIR.replace("@phase lowered", &format!("@phase {phase}"))).unwrap();
        let pipeline = format!("-Zmir-pipeline=example::{pass}");
        let output =
            run(&path, &sample, &["-Ogas", &pipeline, "-Zprint-after-each", "-Zvalidate-ir=false"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        snapshots.push(text.replace(&path.display().to_string(), "$INPUT"));
        if phase == "lowered" {
            let program = &text[text.find("@module").unwrap()..];
            for value in [
                alloy_primitives::U256::ZERO,
                alloy_primitives::U256::from(1),
                alloy_primitives::U256::from(123),
                alloy_primitives::U256::MAX,
            ] {
                let input = value.to_be_bytes::<32>();
                let result = solar_mir_interpreter::transact(
                    program,
                    &input,
                    &mut EmptyHost,
                    Default::default(),
                )
                .unwrap();
                assert_eq!(result.outcome, solar_mir_interpreter::Outcome::Success(input.to_vec()));
            }
            let compiled = run(&path, &sample, &["-Ogas", &pipeline, "--emit=bin-runtime"]);
            assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
            fs::write(
                &path,
                MIR.replace("add v0, 0", "add v0, 0 !metadata(span=10..20)")
                    .replace("returndata 0, 32", "returndata 0, 32 !metadata(span=20..30)"),
            )
            .unwrap();
            let debug = run(&path, &sample, &["-Ogas", &pipeline, "--emit=bin-runtime"]);
            assert!(debug.status.success(), "{}", String::from_utf8_lossy(&debug.stderr));
            let compiled = artifact(&compiled);
            let debug = artifact(&debug);
            assert_eq!(compiled["contracts"], debug["contracts"]);
            for value in [
                alloy_primitives::U256::ZERO,
                alloy_primitives::U256::from(123),
                alloy_primitives::U256::MAX,
            ] {
                let input = value.to_be_bytes::<32>();
                assert_eq!(super::runtime::execute(&compiled, &input), input.to_vec());
            }
        }
    }
    assert_data_eq!(
        snapshots.join("\n"),
        str![[r#"
; === $INPUT (after example::create-mir-helper) ===
@module PluginTest
; runtime dispatch entry
fn @entry() [entry] {
  bb0:
    v0 = calldataload 0
    jump bb1
  bb1:                                            ; preds = bb0
    v2 = add v0, 0
    v3 = icall @plugin_helper_1, v2
    mstore 0, v3
    returndata 0, 32
}

fn @plugin_helper_1(arg0: i256) -> i256 {
  bb0:
    v0 = eq arg0, 0
    jumpi v0, bb1, bb2
  bb2:                                            ; preds = bb0
    jump bb3
  bb1:                                            ; preds = bb0
    jump bb3
  bb3:                                            ; preds = bb1, bb2
    v1 = phi [bb1: arg0], [bb2: arg0]
    v2 = icall @plugin_leaf_2, v1
    jump bb4
  bb4:                                            ; preds = bb3
    ret v2
}

fn @plugin_leaf_2(arg0: i256) -> i256 {
  bb0:
    jump bb1
  bb1:                                            ; preds = bb0, bb2
    v0 = phi [bb0: 0], [bb2: v2]
    v1 = lt v0, 2
    jumpi v1, bb2, bb3
  bb3:                                            ; preds = bb1
    ret arg0
  bb2:                                            ; preds = bb1
    v2 = add v0, 1
    jump bb1
}

; === $INPUT (after example::create-lowered-helper) ===
@module PluginTest
@phase lowered
; runtime dispatch entry
fn @entry() [entry] {
  bb0:
    v0 = calldataload 0
    jump bb1
  bb1:                                            ; preds = bb0
    v2 = add v0, 0
    v3 = icall @plugin_helper_1, v2
    mstore 0, v3
    returndata 0, 32
}

fn @plugin_helper_1(arg0: i256) -> i256 {
  bb0:
    v0 = eq arg0, 0
    jumpi v0, bb1, bb2
  bb2:                                            ; preds = bb0
    jump bb3
  bb1:                                            ; preds = bb0
    jump bb3
  bb3:                                            ; preds = bb1, bb2
    v1 = phi [bb1: arg0], [bb2: arg0]
    v2 = icall @plugin_leaf_2, v1
    jump bb4
  bb4:                                            ; preds = bb3
    ret v2
}

fn @plugin_leaf_2(arg0: i256) -> i256 {
  bb0:
    jump bb1
  bb1:                                            ; preds = bb0, bb2
    v0 = phi [bb0: 0], [bb2: v2]
    v1 = lt v0, 2
    jumpi v1, bb2, bb3
  bb3:                                            ; preds = bb1
    ret arg0
  bb2:                                            ; preds = bb1
    v2 = add v0, 1
    jump bb1
}

"#]]
    );
}

#[test]
fn plugins_create_evm_callable_helpers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.evmir");
    fs::write(&path, EVM).unwrap();
    let pipeline = "-Zevm-ir-pipeline=example::create-evm-helper";
    let output =
        run(&path, &sample(), &["-Ogas", pipeline, "-Zprint-after-each", "-Zvalidate-ir=false"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let program = &text[text.find("@module").unwrap()..];
    for length in [0, 1, 31, 32, 129] {
        let input = vec![42; length];
        let result =
            solar_evmir_interpreter::transact(program, &input, &mut EmptyHost, Default::default())
                .unwrap();
        let expected = alloy_primitives::U256::from(length).to_be_bytes::<32>().to_vec();
        assert_eq!(result.outcome, solar_evmir_interpreter::Outcome::Success(expected));
    }
    let compiled = run(&path, &sample(), &["-Ogas", pipeline, "--emit=bin-runtime"]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    fs::write(
        &path,
        EVM.replace(" not\n", " not !metadata(span=10..20)\n")
            .replace(" return\n", " return !metadata(span=20..30)\n"),
    )
    .unwrap();
    let debug = run(&path, &sample(), &["-Ogas", pipeline, "--emit=bin-runtime"]);
    assert!(debug.status.success(), "{}", String::from_utf8_lossy(&debug.stderr));
    let compiled: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    let debug: Value = serde_json::from_slice(&debug.stdout).unwrap();
    assert_eq!(compiled["contracts"], debug["contracts"]);
    for length in [0, 1, 32, 129] {
        let expected = alloy_primitives::U256::from(length).to_be_bytes::<32>().to_vec();
        assert_eq!(super::runtime::execute(&compiled, &vec![42; length]), expected);
    }
    assert_data_eq!(
        text.replace(&path.display().to_string(), "$INPUT"),
        str![[r#"
; === $INPUT (after example::create-evm-helper) ===
@module PluginTest
bb0:
  calldatasize
  push bb1
  swap 1
  jump bb2
bb1:
  push 0
  mstore
  push 32
  push 0
  return
bb2 [ret=1, inputs=2, outputs=1]:
  not
  not
  swap 1
  jump

"#]]
    );
}
