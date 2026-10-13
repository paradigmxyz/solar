use super::*;

#[test]
fn rejects_import_alias_captured_by_a_contract_type() {
    let fixture = RequestFixture::new(
        r#"
        //- /Types.sol
        contract Original {}
        //- /Main.sol
        import {Original as $1Imported} from "./Types.sol";
        contract C {
            struct Local { uint256 value; }
            $2Imported target;
        }
        "#,
        "/Main.sol",
    );

    for marker in ["$1", "$2"] {
        fixture.check_rename_error(marker, "Local", ErrorCode::REQUEST_FAILED);
    }
}

#[test]
fn rejects_inherited_state_capture_in_any_merged_batch() {
    let source = r#"
        //- /Shared.sol
        contract Base {
            uint256 $1balance;
        }
        //- /First.sol
        import "./Shared.sol";
        contract First is Base {
            function read() public view returns (uint256) { return $2balance; }
        }
        //- /Second.sol
        import "./Shared.sol";
        contract Second is Base {
            function read(uint256 amount) public view returns (uint256) {
                return $3balance + amount;
            }
        }
        "#;

    for paths in [["/First.sol", "/Second.sol"], ["/Second.sol", "/First.sol"]] {
        let fixture = RequestFixture::new_in_batches(source, &paths);
        for marker in ["$1", "$2", "$3"] {
            fixture.check_rename_error(marker, "amount", ErrorCode::REQUEST_FAILED);
        }
    }
}

#[test]
fn allows_qualified_imported_type_despite_same_named_local_type() {
    let fixture = RequestFixture::new(
        r#"
        //- /Types.sol
        contract $1Original {}
        //- /Main.sol
        import "./Types.sol" as Types;
        contract C {
            struct Local { uint256 value; }
            Types.$2Original target;
        }
        "#,
        "/Main.sol",
    );

    fixture.check_renames(
        &[("$1 $2", "Local")],
        str![[r#"
$1 $2:
/Main.sol:3:10-3:18 -> Local
/Types.sol:0:9-0:17 -> Local

"#]],
    );
}
