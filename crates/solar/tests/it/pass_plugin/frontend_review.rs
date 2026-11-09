//! Regressions for frontend registration, expansion boundaries, and expression integration.

use super::*;

#[test]
fn declared_ir_passes_require_an_implementation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    fs::write(&path, "contract C { function f() external {} }").unwrap();
    let output = run(&path, &broken(directory.path(), "missing_run"), &["--emit=bin-runtime"]);
    assert!(!output.status.success());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "plugin pass `broken::fail` failed: plugin does not implement IR passes",
  "aborting due to 1 previous error"
]
"#]]
    );
}

#[test]
fn registration_rejects_invalid_metadata_before_parsing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    fs::write(&path, "contract C {}").unwrap();
    let mut errors = Vec::new();
    for mode in [
        "frontend_bad_api",
        "frontend_empty_version",
        "frontend_keyword_namespace",
        "frontend_keyword_extension",
        "frontend_keyword_builtin",
        "frontend_fixed_name",
    ] {
        let output = run(&path, &broken(directory.path(), mode), &["--stop-after", "parsing"]);
        assert!(!output.status.success(), "{mode}");
        errors.push((mode, diagnostic_messages(&output)));
    }
    assert_data_eq!(
        serde_json::to_string_pretty(&errors).unwrap(),
        str![[r#"
[
  [
    "frontend_bad_api",
    [
      "plugin `broken` requires API 999, expected 1",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_empty_version",
    [
      "plugin `broken` has an empty version",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_keyword_namespace",
    [
      "invalid frontend plugin namespace `contract`",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_keyword_extension",
    [
      "invalid or duplicate frontend extension `@broken.if`",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_keyword_builtin",
    [
      "invalid or duplicate plugin builtin `@broken.while`",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_fixed_name",
    [
      "invalid or duplicate plugin builtin `@broken.fixed128x18`",
      "aborting due to 1 previous error"
    ]
  ]
]
"#]]
    );
}

#[test]
fn plugin_expressions_accept_statement_continuations() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    fs::write(
        &path,
        r#"contract C {
        function value() external pure returns(uint) { return 3; }
        function f() external view returns(uint) {
            uint[] memory xs = new uint[](1);
            @broken.id(xs)[0] = 7;
            @broken.id(xs)[0]++;
            @broken.id(xs).length;
            @broken.id(this).value();
            @broken.id(1) + 2;
            @broken.id(xs)[0] == 8;
            @broken.id(true) ? xs[0] : 0;
            return xs[0];
        }
    }"#,
    )
    .unwrap();
    let plugin = broken(directory.path(), "frontend_identity");
    for optimization in ["-Onone", "-Ogas", "-Osize"] {
        let output = run(&path, &plugin, &[optimization, "--emit=bin-runtime"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            runtime::execute(&artifact(&output), &alloy_primitives::keccak256("f()")[..4]),
            alloy_primitives::U256::from(8).to_be_bytes::<32>()
        );
    }
}

#[test]
fn builtin_opcodes_respect_the_selected_fork() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    fs::write(&path, "contract C { function f(uint x) external pure returns(uint) { return @broken.bad(x, 0); } }").unwrap();
    let plugin = broken(directory.path(), "frontend_clz");
    let output =
        run(&path, &plugin, &["--evm-version=prague", "-Zvalidate-ir=false", "--emit=bin-runtime"]);
    assert!(!output.status.success());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "plugin builtin `@broken.bad` failed: plugin builtin opcode `clz` is not available for `prague`",
  "aborting due to 1 previous error"
]
"#]]
    );
    let output = run(&path, &plugin, &["--evm-version=osaka", "-Ogas", "--emit=bin-runtime"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut input = alloy_primitives::keccak256("f(uint256)")[..4].to_vec();
    input.extend_from_slice(&alloy_primitives::U256::from(1).to_be_bytes::<32>());
    assert_eq!(
        runtime::execute(&artifact(&output), &input),
        alloy_primitives::U256::from(255).to_be_bytes::<32>()
    );
}

#[test]
fn composed_expansions_obey_the_total_ast_depth_limit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    for (mode, body) in [
        (
            "frontend_composed_depth",
            format!("return {}1{};", "@broken.deep(".repeat(8), ")".repeat(8)),
        ),
        (
            "frontend_composed_statements",
            format!("{}{}", "@broken.repeat() {".repeat(8), "}".repeat(8)),
        ),
        ("frontend_composed_depth", format!("return @broken.deep(1){};", " + 0".repeat(200))),
        ("frontend_composed_depth", format!("return {}@broken.deep(1);", "- ".repeat(200))),
        ("frontend_composed_depth", format!("identity(@broken.deep(1)){};", " + 0".repeat(200))),
    ] {
        let simple = if mode == "frontend_composed_depth" {
            "return @broken.deep(1);"
        } else {
            "@broken.repeat() {}"
        };
        let plugin = broken(directory.path(), mode);
        fs::write(&path, format!("contract C {{ function f() external {{ {simple} }} }}")).unwrap();
        let output = run(&path, &plugin, &["--stop-after", "parsing"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        fs::write(&path, format!("contract C {{ function f() external {{ {body} }} }}")).unwrap();
        let output = run(&path, &plugin, &["--stop-after", "parsing"]);
        assert!(!output.status.success(), "{mode}: {body}");
        assert_data_eq!(
            messages(&output),
            str![[r#"
[
  "expanded plugin AST exceeds nesting limit",
  "aborting due to 1 previous error"
]
"#]]
        );
    }
}

#[test]
fn deep_recipes_are_rejected_before_cloning() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    for (mode, body) in [
        ("frontend_clone_depth", "return @broken.deep(1);"),
        ("frontend_clone_statements", "@broken.repeat() {}"),
    ] {
        fs::write(&path, format!("contract C {{ function f() external {{ {body} }} }}")).unwrap();
        let output = run(&path, &broken(directory.path(), mode), &["--stop-after", "parsing"]);
        assert!(!output.status.success(), "{mode}");
        assert_data_eq!(
            messages(&output),
            str![[r#"
[
  "plugin expansion exceeds nesting limit",
  "aborting due to 1 previous error"
]
"#]]
        );
    }
}

#[test]
fn wide_recipes_are_rejected_before_cloning() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sol");
    fs::write(&path, "contract C { function f() external { @broken.repeat() {} } }").unwrap();
    let output =
        run(&path, &broken(directory.path(), "frontend_clone_wide"), &["--stop-after", "parsing"]);
    assert!(!output.status.success());
    assert_data_eq!(
        messages(&output),
        str![[r#"
[
  "plugin expansion exceeds node limit",
  "aborting due to 1 previous error"
]
"#]]
    );
}
