use super::*;
use crate::symbols::SymbolTablesAggregator;

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

#[test]
fn preserves_binding_snapshots_before_and_after_nested_merges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Shared.sol
        contract Base {
            uint256 $1balance;
        }
        //- /First.sol
        import "./Shared.sol";
        contract First is Base {
            function read() public view returns (uint256) { return balance; }
        }
        //- /Second.sol
        import "./Shared.sol";
        contract Second is Base {
            function read(uint256 amount) public view returns (uint256) {
                return balance + amount;
            }
        }
        //- /Third.sol
        import "./Shared.sol";
        contract Third is Base {
            function read() public view returns (uint256) { return balance; }
        }
        "#,
        "/First.sol",
    );
    let (uri, position) = fixture.marker_location("$1");

    for (warm, merged_first) in [(false, false), (false, true), (true, false), (true, true)] {
        let originals = ["/First.sol", "/Second.sol", "/Third.sol"].map(|path| {
            let result = analyze_source(fixture.project_path(path), fixture.project_contents(path));
            assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
            result.symbol_tables
        });
        assert!(originals.iter().all(|tables| !tables.rename_bindings_initialized()));
        if warm {
            let mut candidate = originals[0].rename_candidate(&uri, position).unwrap();
            originals[0].validate_rename_bindings(&mut candidate, "storedAmount").unwrap();
        }

        let mut aggregator = SymbolTablesAggregator::default();
        aggregator.push(originals[0].clone());
        aggregator.push(originals[1].clone());
        let pair = aggregator.finish();
        assert!(originals[..2].iter().all(|tables| tables.rename_bindings_initialized()));
        assert!(!originals[2].rename_bindings_initialized());

        let mut aggregator = SymbolTablesAggregator::default();
        if merged_first {
            aggregator.push(pair);
            aggregator.push(originals[2].clone());
        } else {
            aggregator.push(originals[2].clone());
            aggregator.push(pair);
        }
        let merged = aggregator.finish();
        assert!(originals.iter().all(|tables| tables.rename_bindings_initialized()));
        assert!(merged.rename_bindings_initialized());

        // Aggregation must not replace the original batches with their combined bindings.
        for (tables, captures) in originals.iter().chain([&merged]).zip([false, true, false, true])
        {
            let mut candidate = tables.rename_candidate(&uri, position).unwrap();
            tables.validate_rename_bindings(&mut candidate, "storedAmount").unwrap();
            let mut candidate = tables.rename_candidate(&uri, position).unwrap();
            let result = tables.validate_rename_bindings(&mut candidate, "amount");
            assert_eq!(
                result.map_err(|error| error.code),
                if captures { Err(ErrorCode::REQUEST_FAILED) } else { Ok(()) },
            );
        }
    }
}
