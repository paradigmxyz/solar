use super::*;
use alloy_primitives::U256;

#[test]
fn repeat_executes_with_once_only_counts_and_loop_control() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("repeat.sol");
    for (source, expected) in [
        (
            "uint calls; function next() internal returns(uint) { ++calls; return 3; } function f() external returns(uint) { uint n; @example.repeat(next()) { ++n; } return calls * 100 + n; }",
            103,
        ),
        (
            "function f() external pure returns(uint) { uint n; uint x; @example.repeat(5) { ++x; if (x == 2) continue; if (x == 4) break; @example.repeat(2) { ++n; } } return n * 10 + x; }",
            44,
        ),
        (
            "function f() external pure returns(uint) { @example.repeat(0) { return 1; } @example.repeat(2) { return 99; } return 0; }",
            99,
        ),
        (
            "uint calls; function next(uint value) internal returns(uint) { ++calls; return value; } function f() external returns(uint) { uint rotated = @example.rotl(next(1), next(1)); return rotated + calls * 10; }",
            22,
        ),
        (
            "uint calls; function next() internal returns(uint) { ++calls; return 1; } function f() external returns(uint) { @example.rotl(next(), next()); return calls; }",
            2,
        ),
        (
            "function f() external pure returns(uint) { uint8 value; uint8 shift; assembly { value := 257 shift := 257 } return @example.rotl(value, shift); }",
            2,
        ),
    ] {
        fs::write(&path, format!("contract C {{ {source} }}")).unwrap();
        for optimization in ["-Onone", "-Ogas", "-Osize"] {
            let output =
                run(&path, &sample(), &[optimization, "--emit=bin-runtime", "-Zvalidate-ir=true"]);
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(
                runtime::execute(&artifact(&output), &alloy_primitives::keccak256("f()")[..4]),
                U256::from(expected).to_be_bytes::<32>()
            );
        }
    }
}

#[test]
fn typed_builtin_emits_and_executes_plugin_mir() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rotl.sol");
    fs::write(&path, "contract C { function f(uint256 value, uint256 shift) external pure returns(uint256) { return @example.rotl(value, shift); } }").unwrap();
    for optimization in ["-Onone", "-Ogas", "-Osize"] {
        let output =
            run(&path, &sample(), &[optimization, "--emit=bin-runtime", "-Zvalidate-ir=true"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        for (value, shift) in [
            (U256::ZERO, 0),
            (U256::from(1), 0),
            (U256::from(1), 255),
            (U256::from(1), 256),
            (U256::from(1) << 255, 1),
            (U256::MAX, 300),
        ] {
            let mut input = alloy_primitives::keccak256("f(uint256,uint256)")[..4].to_vec();
            input.extend_from_slice(&value.to_be_bytes::<32>());
            input.extend_from_slice(&U256::from(shift).to_be_bytes::<32>());
            let count = shift % 256;
            let expected =
                if count == 0 { value } else { (value << count) | (value >> (256 - count)) };
            assert_eq!(runtime::execute(&artifact(&output), &input), expected.to_be_bytes::<32>());
        }
    }
}

#[test]
fn invalid_builtin_fragments_are_rejected_with_validation_disabled() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bad.sol");
    fs::write(&path, "contract C { function f(uint x) external pure returns(uint) { return @broken.bad(x, 1); } }").unwrap();
    let mut errors = Vec::new();
    for mode in [
        "frontend_local",
        "frontend_argument",
        "frontend_gas",
        "frontend_memory",
        "frontend_arity",
        "frontend_unknown",
        "frontend_effect",
    ] {
        let output = run(
            &path,
            &broken(directory.path(), mode),
            &["-Onone", "--emit=bin", "-Zvalidate-ir=false"],
        );
        assert!(!output.status.success(), "{mode} unexpectedly compiled");
        assert!(output.stdout.is_empty(), "invalid builtin emitted bytecode");
        errors.push((mode, diagnostic_messages(&output)));
    }
    assert_data_eq!(
        serde_json::to_string_pretty(&errors).unwrap(),
        str![[r#"
[
  [
    "frontend_local",
    [
      "plugin builtin `@broken.bad` failed: builtin local must refer to a prior instruction",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_argument",
    [
      "plugin builtin `@broken.bad` failed: invalid builtin argument index",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_gas",
    [
      "plugin builtin `@broken.bad` failed: plugin builtin operation is not a pure word opcode",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_memory",
    [
      "plugin builtin `@broken.bad` failed: unknown operation or invalid operand count",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_arity",
    [
      "plugin builtin `@broken.bad` failed: unknown operation or invalid operand count",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_unknown",
    [
      "plugin builtin `@broken.bad` failed: unknown operation or invalid operand count",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_effect",
    [
      "plugin builtins require an explicit pure effect",
      "aborting due to 1 previous error"
    ]
  ]
]
"#]]
    );
}

#[test]
fn frontend_diagnostics_use_source_locations_and_types() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bad.sol");
    let mut errors = Vec::new();
    for (name, body) in [
        ("type", "return @example.rotl(true, 1);"),
        ("arity", "return @example.rotl(1);"),
        ("unknown", "return @example.unknown(1);"),
        ("repeat_type", "@example.repeat(true) {} return 0;"),
    ] {
        fs::write(
            &path,
            format!("contract C {{ function f() external pure returns(uint) {{ {body} }} }}"),
        )
        .unwrap();
        let output = run(&path, &sample(), &["--emit=bin"]);
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        let diagnostics = String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|value| value["level"] == "error")
            .collect::<Vec<_>>();
        errors.push((
            name,
            diagnostics
                .iter()
                .map(|value| {
                    (
                        value["message"].clone(),
                        value["spans"].as_array().map(|spans| {
                            spans
                                .iter()
                                .map(|span| (span["byte_start"].clone(), span["byte_end"].clone()))
                                .collect::<Vec<_>>()
                        }),
                    )
                })
                .collect::<Vec<_>>(),
        ));
    }
    assert_data_eq!(
        serde_json::to_string_pretty(&errors).unwrap(),
        str![[r#"
[
  [
    "type",
    [
      [
        "mismatched types",
        [
          [
            77,
            81
          ]
        ]
      ],
      [
        "aborting due to 1 previous error",
        []
      ]
    ]
  ],
  [
    "arity",
    [
      [
        "wrong number of plugin builtin arguments",
        [
          [
            63,
            79
          ]
        ]
      ],
      [
        "aborting due to 1 previous error",
        []
      ]
    ]
  ],
  [
    "unknown",
    [
      [
        "unknown frontend extension `@example.unknown`",
        [
          [
            63,
            82
          ]
        ]
      ],
      [
        "aborting due to 1 previous error",
        []
      ]
    ]
  ],
  [
    "repeat_type",
    [
      [
        "mismatched types",
        [
          [
            72,
            76
          ]
        ]
      ],
      [
        "aborting due to 1 previous error",
        []
      ]
    ]
  ]
]
"#]]
    );
}

#[test]
fn nested_statement_recipes_bind_all_arguments_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested.sol");
    fs::write(&path, "contract C { uint calls; function next(uint value) internal returns(uint) { ++calls; return value; } function f() external returns(uint) { uint n; @broken.repeat(next(2), next(3)) { ++n; } return calls * 100 + n; } }").unwrap();
    for optimization in ["-Onone", "-Ogas", "-Osize"] {
        let output = run(
            &path,
            &broken(directory.path(), "frontend_nested"),
            &[optimization, "--emit=bin-runtime"],
        );
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            runtime::execute(&artifact(&output), &alloy_primitives::keccak256("f()")[..4]),
            U256::from(206).to_be_bytes::<32>()
        );
    }
}

#[test]
fn statement_recipes_cannot_duplicate_or_discard_the_body() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("body.sol");
    fs::write(&path, "contract C { function f() external { @broken.repeat() {} } }").unwrap();
    let mut errors = Vec::new();
    for mode in ["frontend_body", "frontend_discard"] {
        let output = run(&path, &broken(directory.path(), mode), &["--emit=bin"]);
        assert!(!output.status.success());
        errors.push((mode, diagnostic_messages(&output)));
    }
    assert_data_eq!(
        serde_json::to_string_pretty(&errors).unwrap(),
        str![[r#"
[
  [
    "frontend_body",
    [
      "plugin expansion duplicates its body",
      "aborting due to 1 previous error"
    ]
  ],
  [
    "frontend_discard",
    [
      "plugin expansion must use every argument and its body once",
      "aborting due to 1 previous error"
    ]
  ]
]
"#]]
    );
}

#[test]
fn a_frontend_only_library_needs_no_ir_pass() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("only.sol");
    for (source, mode) in
        [("@broken.bad(20, 22)", "frontend_only"), ("@broken.bad()", "frontend_constant")]
    {
        fs::write(
            &path,
            format!(
                "contract C {{ function f() external pure returns(uint) {{ return {source}; }} }}"
            ),
        )
        .unwrap();
        let output = run(&path, &broken(directory.path(), mode), &["-Ogas", "--emit=bin-runtime"]);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            runtime::execute(&artifact(&output), &alloy_primitives::keccak256("f()")[..4]),
            U256::from(42).to_be_bytes::<32>()
        );
    }
}
