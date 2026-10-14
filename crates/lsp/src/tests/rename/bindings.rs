use super::*;

mod cross_file;

#[test]
fn rejects_state_variable_captured_by_parameter() {
    let fixture = RequestFixture::new(
        r#"
        //- /Capture.sol open
        contract C {
            uint256 $1balance;
            function f(uint256 amount) public view returns (uint256) {
                return $2balance + amount;
            }
        }
        "#,
        "/Capture.sol",
    );

    for marker in ["$1", "$2"] {
        fixture.check_rename_error(marker, "amount", ErrorCode::REQUEST_FAILED);
    }
}

#[test]
fn rejects_parameter_capturing_an_unedited_state_reference() {
    let fixture = RequestFixture::new(
        r#"
        //- /Capture.sol
        contract C {
            uint256 balance;
            function f(uint256 $1amount) public view returns (uint256) {
                return balance + $2amount;
            }
        }
        "#,
        "/Capture.sol",
    );

    for marker in ["$1", "$2"] {
        fixture.check_rename_error(marker, "balance", ErrorCode::REQUEST_FAILED);
    }
}

#[test]
fn allows_same_name_in_an_unrelated_function() {
    let fixture = RequestFixture::new(
        r#"
        //- /Safe.sol
        contract C {
            uint256 $1balance;
            function read() public view returns (uint256) { return balance; }
            function f(uint256 amount) public pure returns (uint256) { return amount; }
        }
        "#,
        "/Safe.sol",
    );

    fixture.check_rename(
        "$1",
        "amount",
        str![[r#"
/Safe.sol:1:12-1:19 -> amount
/Safe.sol:2:59-2:66 -> amount

"#]],
    );
}

#[test]
fn allows_qualified_getter_despite_same_named_parameter() {
    let fixture = RequestFixture::new(
        r#"
        //- /Safe.sol
        contract C {
            uint256 public $1balance;
            function f(uint256 amount) public view returns (uint256) {
                return this.balance() + amount;
            }
        }
        "#,
        "/Safe.sol",
    );

    fixture.check_rename(
        "$1",
        "amount",
        str![[r#"
/Safe.sol:1:19-1:26 -> amount
/Safe.sol:3:20-3:27 -> amount

"#]],
    );
}

#[test]
fn rejects_collision_between_unused_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Collision.sol
        contract C {
            uint256 $1balance;
            uint256 amount;
        }
        "#,
        "/Collision.sol",
    );

    fixture.check_rename_error("$1", "amount", ErrorCode::REQUEST_FAILED);
}

#[test]
fn rejects_capture_of_a_builtin_function() {
    let fixture = RequestFixture::new(
        r#"
        //- /Builtin.sol
        contract C {
            function $1customHash(bytes memory) internal pure returns (bytes32) {
                return bytes32(0);
            }
            function f(bytes memory data) public pure returns (bytes32) {
                return keccak256(data);
            }
        }
        "#,
        "/Builtin.sol",
    );

    fixture.check_rename_error("$1", "keccak256", ErrorCode::REQUEST_FAILED);
}

#[test]
fn rejects_overload_that_captures_an_existing_call() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overload.sol
        contract C {
            function $1first(uint256 x) internal pure returns (uint256) { return x; }
            function second(uint8 x) internal pure returns (uint256) { return x; }
            function f(uint256 x) public pure returns (uint256) {
                return first(x) + second(1);
            }
        }
        "#,
        "/Overload.sol",
    );

    fixture.check_rename_error("$1", "second", ErrorCode::REQUEST_FAILED);
}

#[test]
fn preserves_positions_after_multiple_length_changing_edits() {
    for newline in ["\n", "\r\n"] {
        let source = format!(
            "//- /Positions.sol open\ncontract C {{{newline}/* 😀 */ uint $1value;{newline}function f() public view returns (uint) {{ return $2value + value; }}{newline}}}{newline}",
        );
        let fixture = RequestFixture::new(&source, "/Positions.sol");
        for name in ["longerName", "y"] {
            let (mut state, params) = fixture.rename_state_and_params("$1", name);
            let edit = block_on(crate::handlers::rename(&mut state, params))
                .unwrap()
                .expect("safe rename should produce edits");
            let changes = edit.changes.unwrap();
            assert_eq!(changes[&fixture.marker_location("$1").0].len(), 3);
        }
    }
}

#[test]
fn allows_safe_rename_with_an_unrelated_existing_error() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Error.sol
        contract C {
            uint256 $1balance;
            function f() public view returns (uint256) { return balance; }
            function broken() public { missing(); }
        }
        "#,
        "/Error.sol",
    );

    fixture.check_rename(
        "$1",
        "storedAmount",
        str![[r#"
/Error.sol:1:12-1:19 -> storedAmount
/Error.sol:2:56-2:63 -> storedAmount

"#]],
    );
}

#[test]
fn rejects_changed_unedited_dependency_used_for_binding_validation() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract Base {}
        //- /Main.sol open
        import "./Base.sol";
        contract C is Base {
            uint256 $1balance;
            function f() public view returns (uint256) { return balance; }
        }
        "#,
        "/Main.sol",
    );
    fixture.write_file("/Base.sol", "contract Base { uint256 amount; }");

    fixture.check_rename_error("$1", "amount", ErrorCode::CONTENT_MODIFIED);
}

#[test]
fn initializes_bindings_on_rename_and_shares_cloned_snapshots() {
    let fixture = RequestFixture::new(
        r#"
        //- /Lazy.sol
        contract C {
            uint256 $1balance;
            function f(uint256 amount) public view returns (uint256) {
                return balance + amount;
            }
        }
        "#,
        "/Lazy.sol",
    );
    let state = fixture.state();
    let tables = state.symbol_tables.load_full();
    let cloned = tables.as_ref().clone();
    assert!(!tables.rename_bindings_initialized());
    assert!(!cloned.rename_bindings_initialized());

    fixture.check_prepare_rename("$1", "1:12-1:19\n");
    assert!(!tables.rename_bindings_initialized());

    fixture.check_rename(
        "$1",
        "storedAmount",
        str![[r#"
/Lazy.sol:1:12-1:19 -> storedAmount
/Lazy.sol:3:15-3:22 -> storedAmount

"#]],
    );
    assert!(tables.rename_bindings_initialized());
    assert!(cloned.rename_bindings_initialized());
    fixture.check_rename_error("$1", "amount", ErrorCode::REQUEST_FAILED);
}
