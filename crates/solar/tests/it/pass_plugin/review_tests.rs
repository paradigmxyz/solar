use super::*;
use solar_pass_plugin::{ExpansionRequest, ExtensionKind, host::NativePlugin};
use std::{io::Write, process::Stdio};

#[test]
fn frontend_callbacks_cross_library_boundary() {
    let plugin = NativePlugin::load(&sample()).unwrap();
    let manifest = plugin.frontend_manifest().unwrap();
    let expansion = plugin
        .expand(&ExpansionRequest {
            name: "repeat".into(),
            kind: ExtensionKind::Statement,
            tokens: Vec::new(),
        })
        .unwrap();
    assert_data_eq!(
        format!("{manifest:?}\n{expansion:?}"),
        str![[
            r#"FrontendManifest { extensions: [FrontendExtension { name: "repeat", kind: Statement }] }
Statement(Repeat { count: Argument(0), body: Body })"#
        ]]
    );
}

#[test]
fn requests_do_not_expose_debug_output_options() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("test.sol");
    fs::write(
        &source,
        "contract C { function f(uint256 x) external pure returns (uint256) { return x + 1; } }",
    )
    .unwrap();
    for mode in ["observe_mir", "observe_evm"] {
        let plugin = broken(directory.path(), mode);
        let mut baseline = None;
        for extra in [
            None,
            Some("-Zdump=mir"),
            Some("-Zdump=evm-ir"),
            Some("-Zdump=evm-ir-runtime"),
            Some("-Zdump=evm-ir-runtime-scheduled"),
            Some("-Zdebugify=report"),
        ] {
            for emit in ["--emit=bin-runtime", "--emit=bin-runtime,srcmap-runtime,ethdebug-runtime"]
            {
                let mut flags = vec!["-Ogas", emit];
                flags.extend(extra);
                let output = run(&source, &plugin, &flags);
                assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
                let snapshots = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .filter_map(|line| line.strip_prefix("SNAPSHOT:"))
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                assert!(!snapshots.is_empty());
                if let Some(baseline) = &baseline {
                    assert_eq!(&snapshots, baseline, "{mode}: {emit} {extra:?}");
                } else {
                    baseline = Some(snapshots);
                }
            }
        }
    }
}

#[test]
fn edits_invalidate_source_memory_safety() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    fs::write(&path, "contract C { function f() external {} }").unwrap();
    let plugin = broken(directory.path(), "raw_write");
    let output =
        run(&path, &plugin, &["-Ogas", "-Zmir-pipeline=broken::fail,broken::fail", "-Zdump=mir"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let flags = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| line.strip_prefix("UNSAFE:"))
        .collect::<Vec<_>>()
        .join(",");
    assert_data_eq!(flags, str![["false,true"]]);
}

#[test]
fn allocation_copy_keeps_semantic_flags() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.mir");
    fs::write(&path, "@module Alloc\nfn @f() -> ptr<memory> {\nbb0:\nv0 = alloc 64 !metadata(deferred_alloc, preserves_fmp)\nret v0\n}\n").unwrap();
    let plugin = broken(directory.path(), "copy_alloc");
    let output = run(&path, &plugin, &["-Zmir-pipeline=broken::fail", "-Zprint-after-each"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_data_eq!(
        String::from_utf8(output.stdout).unwrap().replace(&path.display().to_string(), "$INPUT"),
        str![[r#"
; === $INPUT (after broken::fail) ===
@module Alloc
fn @f() -> ptr<memory> {
  bb0:
    v1 = alloc 64 !metadata(deferred_alloc, preserves_fmp)
    ret v1
}

"#]]
    );
}

#[test]
fn malformed_phi_order_and_helper_returns_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let mut results = Vec::new();
    for (mode, extension, source, pipeline) in [
        (
            "late_phi",
            "mir",
            "@module Phi\nfn @f() {\nbb0:\nret\n}\n",
            "-Zmir-pipeline=broken::fail",
        ),
        ("wrong_returns", "evmir", "@module Calls\nbb0:\nstop\n", "-Zevm-ir-pipeline=broken::fail"),
    ] {
        let path = directory.path().join(format!("test.{extension}"));
        fs::write(&path, source).unwrap();
        let plugin = broken(directory.path(), mode);
        let output = run(&path, &plugin, &[pipeline, "-Zvalidate-ir=false", "--emit=bin"]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        results.push((mode, diagnostic_messages(&output)));
    }
    assert_data_eq!(
        serde_json::to_string_pretty(&results).unwrap(),
        str![[r#"
[
  [
    "late_phi",
    [
      "[fn1] [bb1, inst1] phi must precede non-phi instructions",
      "plugin pass `broken::fail` failed: produced invalid MIR",
      "aborting due to 2 previous errors"
    ]
  ],
  [
    "wrong_returns",
    [
      "plugin pass `broken::fail` failed: helper bb1 return does not match its declared call effect",
      "aborting due to 1 previous error"
    ]
  ]
]
"#]]
    );
}

#[test]
fn plugin_binary_identity_reaches_metadata_and_ethdebug() {
    let directory = tempfile::tempdir().unwrap();
    let plugin = broken(directory.path(), "observe_mir");
    let input = json!({"language":"Solidity", "sources":{"C.sol":{"content":"contract C { function f() external pure returns (uint256) { return 1; } }"}},
        "settings":{"outputSelection":{"*":{"*":["metadata","evm.bytecode.object","evm.deployedBytecode.object","evm.deployedBytecode.ethdebug","ethdebug.resources"]}}}}).to_string();
    let compile = |plugin: Option<&Path>| {
        let mut cmd = Command::new(SOLAR);
        cmd.args(["--standard-json", "--allow=2264"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(plugin) = plugin {
            cmd.arg(format!("-Zload-pass-plugin={}", plugin.display()))
                .env("SOLAR_TEST_PASS_PLUGIN_MODE", "observe_mir");
        }
        let mut child = cmd.spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        let errors = result["errors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|error| error["severity"] == "error")
            .collect::<Vec<_>>();
        assert_data_eq!(serde_json::to_string(&errors).unwrap(), str![["[]"]]);
        result
    };
    let plain = compile(None);
    let loaded = compile(Some(&plugin));
    let metadata: Value =
        serde_json::from_str(loaded["contracts"]["C.sol"]["C"]["metadata"].as_str().unwrap())
            .unwrap();
    // Native plugin artifacts are host files, not virtual Solidity sources.
    #[allow(clippy::disallowed_methods)]
    let digest = alloy_primitives::keccak256(fs::read(&plugin).unwrap());
    assert_eq!(metadata["settings"]["solar"]["plugins"], json!([format!("broken@1:{digest:#x}")]));
    assert_ne!(
        plain["contracts"]["C.sol"]["C"]["metadata"],
        loaded["contracts"]["C.sol"]["C"]["metadata"]
    );
    assert_ne!(
        plain["ethdebug"]["resources"]["compilation"]["id"],
        loaded["ethdebug"]["resources"]["compilation"]["id"]
    );
    let copy = directory.path().join(format!("relocated{}", std::env::consts::DLL_SUFFIX));
    fs::copy(&plugin, &copy).unwrap();
    let relocated = compile(Some(&copy));
    assert_eq!(loaded["contracts"], relocated["contracts"]);
    assert_eq!(loaded["ethdebug"], relocated["ethdebug"]);
}

#[test]
fn edits_preserve_untouched_helpers_with_spilled_returns() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("spilled.evmir");
    fs::write(&path, "@module Spilled\nbb0:\n calldatasize\n not\n not\n pop\n push bb1\n jump bb2\nbb1:\n push 32\n push 0\n return\nbb2 [ret=0, inputs=1, outputs=0]:\n push 128\n mstore\n push 42\n push 0\n mstore\n push 128\n mload\n jump\n").unwrap();
    let output = run(
        &path,
        &sample(),
        &[
            "-Ogas",
            "-Zevm-ir-pipeline=example::create-evm-helper",
            "-Zvalidate-ir=false",
            "--emit=bin-runtime",
        ],
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        super::runtime::execute(&artifact(&output), &[]),
        alloy_primitives::U256::from(42).to_be_bytes::<32>()
    );
}

#[test]
fn editing_a_helper_continuation_rechecks_its_contract() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("continuation.evmir");
    fs::write(&path, "@module Continuation\nbb0:\n push bb1\n jump bb2\nbb1:\n stop\nbb2 [ret=0, inputs=1, outputs=0]:\n push bb3\n jump bb4\nbb3:\n jump\nbb4 [ret=0, inputs=1, outputs=0]:\n jump\n").unwrap();
    let output = run(
        &path,
        &broken(directory.path(), "existing_helper_output"),
        &["-Zevm-ir-pipeline=broken::fail", "-Zvalidate-ir=false", "--emit=bin-runtime"],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "plugin pass `broken::fail` failed: helper bb2 return does not match its declared call effect",
  "aborting due to 1 previous error"
]
"#]]
    );
}

#[test]
fn unrelated_edits_accept_backend_generated_helpers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backend.mir");
    let mut source = "@module Backend\n@phase lowered\nfn @entry() [entry] {\nbb0:\n".to_owned();
    for index in 0..20 {
        source.push_str(&format!("v{index} = calldataload {}\n", index * 32));
    }
    source.push_str(&format!(
        "v20 = icall @sum, {}\nmstore 0, v20\nreturndata 0, 32\n}}\n",
        (0..20).map(|i| format!("v{i}")).collect::<Vec<_>>().join(", ")
    ));
    source.push_str(&format!(
        "fn @sum({}) -> i256 {{\nbb0:\nv0 = add arg0, arg1\n",
        (0..20).map(|i| format!("arg{i}: i256")).collect::<Vec<_>>().join(", ")
    ));
    for index in 2..20 {
        source.push_str(&format!("v{} = add v{}, arg{index}\n", index - 1, index - 2));
    }
    source.push_str("ret v18\n}\n");
    fs::write(&path, source).unwrap();
    let output = run(
        &path,
        &broken(directory.path(), "unused_evm_block"),
        &["-Onone", "-Zmir-pipeline=none", "-Zdump=evm-ir-runtime-scheduled", "--emit=bin-runtime"],
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let input = (0..20)
        .flat_map(|_| alloy_primitives::U256::from(1).to_be_bytes::<32>())
        .collect::<Vec<_>>();
    assert_eq!(
        super::runtime::execute(&artifact(&output), &input),
        alloy_primitives::U256::from(20).to_be_bytes::<32>()
    );
}

#[test]
fn semantic_plugins_can_construct_wrapping_pointer_additions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ptradd.mir");
    fs::write(&path, "@module Wrapping\nfn @entry() [entry] {\nbb0:\nv0 = calldataload 0\nv1 = inttoptr i256 v0 to ptr<memory>\nv2 = calldataload 32\nv3 = icall @offset, v1, v2\nv4 = ptrtoint ptr<memory> v3 to i256\nmstore 0, v4\nreturndata 0, 32\n}\nfn @offset(arg0: ptr<memory>, arg1: i256) -> ptr<memory> {\nbb0:\nv0 = ptradd arg0, 0\nret v0\n}\n").unwrap();
    for optimization in ["-Onone", "-Ogas", "-Osize"] {
        let output = run(
            &path,
            &broken(directory.path(), "semantic_ptradd"),
            &[
                optimization,
                "-Zmir-pipeline=broken::fail,lower-memory-objects,lower-evm-shaped",
                "--emit=bin-runtime",
                "-Zvalidate-ir=false",
            ],
        );
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        for (base, offset) in [
            (alloy_primitives::U256::from(0x100), alloy_primitives::U256::MAX),
            (alloy_primitives::U256::MAX, alloy_primitives::U256::from(33)),
        ] {
            let mut input = base.to_be_bytes::<32>().to_vec();
            input.extend_from_slice(&offset.to_be_bytes::<32>());
            assert_eq!(
                super::runtime::execute(&artifact(&output), &input),
                base.wrapping_add(offset).to_be_bytes::<32>()
            );
        }
    }
}

#[test]
fn mir_edits_invalidate_transitive_array_widths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("arrays.mir");
    fs::write(
        &path,
        r#"@module Arrays
fn @write(arg0: ptr<memory>) {
  bb0:
    v0 = ptradd arg0, 32
    mstore v0, 1
    ret
}
fn @fresh() -> ptr<memory> [selector=0x00000001, abi_returns=[memory_array<word<uint8>>], abi_return_params=[array<_, uint8>]] {
  bb0:
    v0 = alloc zeroed checked 64
    mstore v0, 1
    icall @write, v0
    ret v0
}
fn @echo(arg0: ptr<memory>) -> ptr<memory> [selector=0x00000002, abi_params=[array<_, uint8>], abi_returns=[memory_array<word<uint8>>], abi_return_params=[array<_, uint8>]] {
  bb0:
    icall @write, arg0
    ret arg0
}
"#,
    )
    .unwrap();
    let plugin = broken(directory.path(), "widen_array_store");
    let expected = [32, 1, 1]
        .into_iter()
        .flat_map(|word| alloy_primitives::U256::from(word).to_be_bytes::<32>())
        .collect::<Vec<_>>();
    for optimization in ["-Ogas", "-Osize"] {
        let output = run(
            &path,
            &plugin,
            &[
                optimization,
                "-Zmir-pipeline=element-cleanup,broken::fail,lower-abi,lower-abi-encode,lower-dispatch,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped",
                "--emit=bin-runtime",
                "-Zvalidate-ir=false",
            ],
        );
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let artifact = artifact(&output);
        for (selector, arguments) in [(1_u32, &[][..]), (2, expected.as_slice())] {
            let mut input = selector.to_be_bytes().to_vec();
            input.extend_from_slice(arguments);
            // Both untouched callers must clean the dirty word stored by the edited callee.
            assert_eq!(super::runtime::execute(&artifact, &input), expected);
        }
    }
}

// Windows locks loaded libraries, so this replacement is only possible on Unix.
// The fixture and replacement are host binaries, outside the Solidity source loader.
#[cfg(unix)]
#[test]
#[allow(clippy::disallowed_methods)]
fn plugin_identity_rejects_library_replacement_during_loading() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    fs::write(&path, "contract C { function f() external {} }").unwrap();
    let bytes = fs::read(sample()).unwrap();
    for mode in ["replace_library", "replace_library_manifest"] {
        let plugin = broken(directory.path(), mode);
        let replacement =
            directory.path().join(format!("replacement{}", std::env::consts::DLL_SUFFIX));
        fs::write(&replacement, &bytes).unwrap();
        let output = Command::new(SOLAR)
            .args(["--allow=2264", "-j1", "--error-format=rustc-json", "--emit=bin-runtime"])
            .arg(format!("-Zload-pass-plugin={}", plugin.display()))
            .arg(&path)
            .env("SOLAR_TEST_PASS_PLUGIN_MODE", mode)
            .env("SOLAR_TEST_PLUGIN_PATH", &plugin)
            .env("SOLAR_TEST_PLUGIN_REPLACEMENT", &replacement)
            .output()
            .unwrap();
        assert_eq!(fs::read(&plugin).unwrap(), bytes);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_data_eq!(
            messages(&output),
            str![[r#"
[
  "pass plugin library changed while loading",
  "aborting due to 1 previous error"
]
"#]],
        );
    }
}
