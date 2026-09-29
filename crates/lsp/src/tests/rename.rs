use super::*;
use lsp_types::DocumentChanges;
use snapbox::str;

mod coverage;
mod dependencies;

// ported-from: test/libsolidity/lsp/rename/contract.sol
#[test]
fn renames_contract_references_from_declarations_and_types() {
    let fixture = RequestFixture::new(
        r#"
        //- /Contract.sol
        contract $1ToRename {}
        contract User {
            ToRename public publicVariable;
            ToRename[10] previousContracts;
            mapping(int => ToRename) contractMapping;
            function getContract() public returns ($2ToRename) {
                return new ToRename();
            }
            function setContract(ToRename value) public {
                publicVariable = value;
            }
        }
        "#,
        "/Contract.sol",
    );

    fixture.check_prepare_rename("$2", "5:43-5:51\n");
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/Contract.sol:0:9-0:17 -> Renamed
/Contract.sol:2:4-2:12 -> Renamed
/Contract.sol:3:4-3:12 -> Renamed
/Contract.sol:4:19-4:27 -> Renamed
/Contract.sol:5:43-5:51 -> Renamed
/Contract.sol:6:19-6:27 -> Renamed
/Contract.sol:8:25-8:33 -> Renamed

"#]],
    );
}

// ported-from: test/libsolidity/lsp/rename/function.sol
#[test]
fn renames_function_references_across_call_forms() {
    let fixture = RequestFixture::new(
        r#"
        //- /Function.sol
        contract C {
            function $1renameMe() public pure returns (int) {
                return 1;
            }
            function other() public view {
                renameMe();
                this.renameMe();
            }
        }
        contract Other {
            C c;
            function other() public view {
                c.$2renameMe();
            }
        }
        function free() pure {
            C c;
            c.renameMe();
        }
        "#,
        "/Function.sol",
    );

    fixture.check_prepare_rename("$2", "12:10-12:18\n");
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/Function.sol:1:13-1:21 -> Renamed
/Function.sol:5:8-5:16 -> Renamed
/Function.sol:6:13-6:21 -> Renamed
/Function.sol:12:10-12:18 -> Renamed
/Function.sol:17:6-17:14 -> Renamed

"#]],
    );
}

// ported-from: test/libsolidity/lsp/rename/variable.sol
#[test]
fn renames_variable_references_and_public_getters() {
    let fixture = RequestFixture::new(
        r#"
        //- /Variable.sol
        contract C {
            int public $1renameMe;
            function foo() public returns (int) {
                $2renameMe = 1;
                return this.$3renameMe();
            }
        }
        function freeFunction(C c) view returns (int) {
            return c.$4renameMe();
        }
        "#,
        "/Variable.sol",
    );

    fixture.check_prepare_rename("$3", "4:20-4:28\n");
    fixture.check_renames(
        &[("$1 $2 $3 $4", "Renamed")],
        str![[r#"
$1 $2 $3 $4:
/Variable.sol:1:15-1:23 -> Renamed
/Variable.sol:3:8-3:16 -> Renamed
/Variable.sol:4:20-4:28 -> Renamed
/Variable.sol:8:13-8:21 -> Renamed

"#]],
    );
}

// ported-from: test/libsolidity/lsp/rename/functionCall.sol
#[test]
fn renames_named_call_arguments_with_the_parameter() {
    let fixture = RequestFixture::new(
        r#"
        //- /NamedArgs.sol
        contract C {
            function foo(int $1a, int b, int c) public pure returns (int) {
                return $2a + b + c;
            }
            function bar() public view {
                this.foo({c: 1, b: 2, $3a: 3});
            }
        }
        "#,
        "/NamedArgs.sol",
    );

    fixture.check_rename(
        "$3",
        "Renamed",
        str![[r#"
/NamedArgs.sol:1:21-1:22 -> Renamed
/NamedArgs.sol:2:15-2:16 -> Renamed
/NamedArgs.sol:5:30-5:31 -> Renamed

"#]],
    );
}

#[test]
fn renames_modifiers_and_their_named_and_base_constructor_arguments() {
    let fixture = RequestFixture::new(
        r#"
        //- /NamedModifiers.sol
        contract Base {
            constructor(uint256 $3amount) {}
        }

        contract Child is Base({$4amount: 1}) {
            modifier $5guarded(uint256 $1amount) { _; }
            function run() public $6guarded({$2amount: 1}) {}
        }
        "#,
        "/NamedModifiers.sol",
    );

    fixture.check_renames(
        &[("$1 $2", "value"), ("$3 $4", "value"), ("$5 $6", "check")],
        str![[r#"
$1 $2:
/NamedModifiers.sol:4:29-4:35 -> value
/NamedModifiers.sol:5:35-5:41 -> value
$3 $4:
/NamedModifiers.sol:1:24-1:30 -> value
/NamedModifiers.sol:3:24-3:30 -> value
$5 $6:
/NamedModifiers.sol:4:13-4:20 -> check
/NamedModifiers.sol:5:26-5:33 -> check

"#]],
    );
    fixture.check_prepare_rename("$6", "5:26-5:33\n");
}

#[test]
fn renames_mapping_names_from_generated_getter_signature() {
    let fixture = RequestFixture::new(
        r#"
        //- /MappingNames.sol
        contract C {
            mapping(address $1owner => mapping(address $3spender => uint256 $5balance)) public balances;

            function read() public view returns (uint256) {
                return this.balances({$2owner: msg.sender, $4spender: address(this)});
            }
        }
        "#,
        "/MappingNames.sol",
    );

    fixture.check_renames(
        &[("$2", "account"), ("$4", "delegate"), ("$5", "amount")],
        str![[r#"
$2:
/MappingNames.sol:1:20-1:25 -> account
/MappingNames.sol:3:30-3:35 -> account
$4:
/MappingNames.sol:1:45-1:52 -> delegate
/MappingNames.sol:3:49-3:56 -> delegate
$5:
/MappingNames.sol:1:64-1:71 -> amount

"#]],
    );
    fixture.check_prepare_rename("$5", "1:64-1:71\n");
}

// ported-from: test/libsolidity/lsp/rename/import_directive.sol
#[test]
fn distinguishes_import_aliases_from_imported_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imported.sol
        contract ToRename {}

        contract User {
            ToRename value;
        }

        //- /Main.sol
        import "./Imported.sol" as $1externalFile;
        import {$2ToRename as $3ExternalContract, $4User} from "./Imported.sol";

        contract C {
            $5ExternalContract externalContract;
            $6externalFile.$7ToRename namespacedContract;
            $8User user;
        }
        "#,
        "/Main.sol",
    );

    for (marker, range) in [
        ("$1", "0:27-0:39\n"),
        ("$2", "1:8-1:16\n"),
        ("$3", "1:20-1:36\n"),
        ("$5", "3:4-3:20\n"),
        ("$6", "4:4-4:16\n"),
        ("$7", "4:17-4:25\n"),
        ("$8", "5:4-5:8\n"),
    ] {
        fixture.check_prepare_rename(marker, range);
    }
    fixture.check_renames(
        &[("$1", "Renamed"), ("$3 $5", "Renamed"), ("$2", "Renamed"), ("$4", "Renamed")],
        str![[r#"
$1:
/Main.sol:0:27-0:39 -> Renamed
/Main.sol:4:4-4:16 -> Renamed
$3 $5:
/Main.sol:1:20-1:36 -> Renamed
/Main.sol:3:4-3:20 -> Renamed
$2:
/Imported.sol:0:9-0:17 -> Renamed
/Imported.sol:2:4-2:12 -> Renamed
/Main.sol:1:8-1:16 -> Renamed
/Main.sol:4:17-4:25 -> Renamed
$4:
/Imported.sol:1:9-1:13 -> Renamed
/Main.sol:1:38-1:42 -> Renamed
/Main.sol:5:4-5:8 -> Renamed

"#]],
    );
}

#[test]
fn validates_names_positions_and_yul_locals() {
    let fixture = RequestFixture::new(
        r#"
        //- /Assembly.sol
        $4contract C {
            uint256 $1stored;

            $5constructor() {}

            function run(uint256 $2input) public returns (uint256 output) {
                uint256 height = $6block.number;
                assembly {
                    let $3local := input
                    sstore(stored.slot, add(local, input))
                    output := local
                }
                output += $7  height;
            }
        }
        "#,
        "/Assembly.sol",
    );

    fixture.check_prepare_rename("$1", "1:12-1:18\n");
    fixture.check_renames(
        &[("$1", "renamed"), ("$2", "renamed"), ("$3", "renamed"), ("$1", "stored")],
        str![[r#"
$1:
/Assembly.sol:1:12-1:18 -> renamed
/Assembly.sol:7:19-7:25 -> renamed
$2:
/Assembly.sol:3:25-3:30 -> renamed
/Assembly.sol:6:25-6:30 -> renamed
/Assembly.sol:7:43-7:48 -> renamed
$3:
<none>
$1:
<none>

"#]],
    );
    for marker in ["$3", "$4", "$5", "$6", "$7"] {
        fixture.check_prepare_rename(marker, "<none>\n");
    }
    for name in ["not a name", "256value", "contract", "uint256", "leave"] {
        fixture.check_rename_error("$1", name, ErrorCode::INVALID_PARAMS);
    }
    fixture.check_rename_error("$2", "add", ErrorCode::INVALID_PARAMS);
}

#[test]
fn renames_qualified_type_components_bases_and_layout_expressions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Qualified.sol
        uint256 constant $5BASE = 42;
        contract $1Outer {
            struct $2Inner { uint256 field; }
        }
        contract $3Child is Outer {}

        contract C is Outer {
            Outer.Inner value;
            $4Child.Inner inherited;

            function read() public view returns (Outer.Inner memory) {
                // Outer and Inner in this text must remain unchanged.
                return value;
            }
        }
        contract Layout layout at BASE {}
        "#,
        "/Qualified.sol",
    );

    fixture.check_renames(
        &[("$1", "Renamed"), ("$2", "Renamed"), ("$3 $4", "Renamed"), ("$5", "RENAMED")],
        str![[r#"
$1:
/Qualified.sol:1:9-1:14 -> Renamed
/Qualified.sol:4:18-4:23 -> Renamed
/Qualified.sol:5:14-5:19 -> Renamed
/Qualified.sol:6:4-6:9 -> Renamed
/Qualified.sol:8:41-8:46 -> Renamed
$2:
/Qualified.sol:2:11-2:16 -> Renamed
/Qualified.sol:6:10-6:15 -> Renamed
/Qualified.sol:7:10-7:15 -> Renamed
/Qualified.sol:8:47-8:52 -> Renamed
$3 $4:
/Qualified.sol:4:9-4:14 -> Renamed
/Qualified.sol:7:4-7:9 -> Renamed
$5:
/Qualified.sol:0:17-0:21 -> RENAMED
/Qualified.sol:13:26-13:30 -> RENAMED

"#]],
    );
}

#[test]
fn renames_validated_natspec_parameter_and_return_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /NatSpec.sol
        contract C {
            /// @param $2amount Payment amount.
            function pay(uint256 $1amount) public {}

            /// @return $4result The value.
            function f() public pure returns (uint256 $3result) {
                result = 1;
            }

            /// @param $5output The output.
            function g() public pure returns (uint256 output) { output = 1; }
        }
        "#,
        "/NatSpec.sol",
    );

    fixture
        .check_goto_definition("$2", "/NatSpec.sol:2:25 function pay(uint256 amount) public {}\n");
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/NatSpec.sol:1:15 /// @param amount Payment amount.
/NatSpec.sol:2:25 function pay(uint256 amount) public {}

"#]],
    );
    fixture.check_document_highlights(
        "$2",
        str![[r#"
1:15-1:21 READ
2:25-2:31 WRITE

"#]],
    );
    fixture.check_renames(
        &[("$1", "value"), ("$3", "value"), ("$5", "value")],
        str![[r#"
$1:
/NatSpec.sol:1:15-1:21 -> value
/NatSpec.sol:2:25-2:31 -> value
$3:
/NatSpec.sol:3:16-3:22 -> value
/NatSpec.sol:4:46-4:52 -> value
/NatSpec.sol:5:8-5:14 -> value
$5:
/NatSpec.sol:7:15-7:21 -> value
/NatSpec.sol:8:46-8:52 -> value
/NatSpec.sol:8:56-8:62 -> value

"#]],
    );

    fixture.check_goto_definition(
        "$4",
        "/NatSpec.sol:4:46 function f() public pure returns (uint256 result) {\n",
    );
    fixture.check_references(
        "$4",
        true,
        str![[r#"
/NatSpec.sol:3:16 /// @return result The value.
/NatSpec.sol:4:46 function f() public pure returns (uint256 result) {
/NatSpec.sol:5:8 result = 1;

"#]],
    );
    fixture.check_document_highlights(
        "$4",
        str![[r#"
3:16-3:22 READ
4:46-4:52 WRITE
5:8-5:14 WRITE

"#]],
    );

    fixture.check_goto_definition(
        "$5",
        "/NatSpec.sol:8:46 function g() public pure returns (uint256 output) { output = 1; }\n",
    );
}

#[test]
fn renames_validated_natspec_inheritdoc_and_override_contract_paths() {
    let fixture = RequestFixture::new(
        r#"
        //- /Inheritdoc.sol
        contract $1Base {
            function run() public virtual {}
            fallback() external virtual {}
            receive() external payable virtual {}
        }

        contract Child is Base {
            /// @inheritdoc $2Base
            function run() public override(Base) {}
            fallback() external override(Base) {}
            receive() external payable override(Base) {}
        }
        "#,
        "/Inheritdoc.sol",
    );

    fixture.check_goto_definition("$2", "/Inheritdoc.sol:0:9 contract Base {\n");
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Inheritdoc.sol:0:9 contract Base {
/Inheritdoc.sol:5:18 contract Child is Base {
/Inheritdoc.sol:6:20 /// @inheritdoc Base

"#]],
    );
    fixture.check_document_highlights(
        "$2",
        str![[r#"
0:9-0:13 WRITE
5:18-5:22 READ
6:20-6:24 READ

"#]],
    );
    fixture.check_rename(
        "$1",
        "Parent",
        str![[r#"
/Inheritdoc.sol:0:9-0:13 -> Parent
/Inheritdoc.sol:5:18-5:22 -> Parent
/Inheritdoc.sol:6:20-6:24 -> Parent
/Inheritdoc.sol:7:35-7:39 -> Parent
/Inheritdoc.sol:8:33-8:37 -> Parent
/Inheritdoc.sol:9:40-9:44 -> Parent

"#]],
    );
}

#[test]
fn renames_override_families_but_not_function_typed_parameters() {
    let fixture = RequestFixture::new(
        r#"
        //- /OverrideFamily.sol
        contract Base {
            function $1run() public virtual {}
            modifier $3guard() virtual { _; }
            function $10hook(uint256 x) public virtual returns (uint256) { return x; }
        }

        contract Child is Base {
            function $2run() public override {}
            modifier $4guard() override { _; }
            function call() public $5guard { $6run(); }
            function use(function(uint256) external returns (uint256) $11hook, uint256 x)
                public returns (uint256) { return $12hook(x); }
        }

        abstract contract GetterBase {
            function $7value() external view virtual returns (uint256);
        }

        contract GetterChild is GetterBase {
            uint256 public override $8value;
            function read() external view returns (uint256) { return this.$9value(); }
        }
        "#,
        "/OverrideFamily.sol",
    );

    fixture.check_renames(
        &[
            ("$1 $2 $6", "renamed"),
            ("$3 $4 $5", "checked"),
            ("$7 $8 $9", "amount"),
            ("$11 $12", "callback"),
            ("$10", "renamed"),
        ],
        str![[r#"
$1 $2 $6:
/OverrideFamily.sol:1:13-1:16 -> renamed
/OverrideFamily.sol:6:13-6:16 -> renamed
/OverrideFamily.sol:8:35-8:38 -> renamed
$3 $4 $5:
/OverrideFamily.sol:2:13-2:18 -> checked
/OverrideFamily.sol:7:13-7:18 -> checked
/OverrideFamily.sol:8:27-8:32 -> checked
$7 $8 $9:
/OverrideFamily.sol:13:13-13:18 -> amount
/OverrideFamily.sol:16:28-16:33 -> amount
/OverrideFamily.sol:17:66-17:71 -> amount
$11 $12:
/OverrideFamily.sol:9:62-9:66 -> callback
/OverrideFamily.sol:10:42-10:46 -> callback
$10:
/OverrideFamily.sol:3:13-3:17 -> renamed

"#]],
    );
}

#[test]
fn renames_members_using_paths_and_attached_calls() {
    let fixture = RequestFixture::new(
        r#"
        //- /Members.sol
        library $3Lib {
            function $4add(uint256 self, uint256 other) internal pure returns (uint256) {
                return self + other;
            }
        }

        using {Lib.add} for uint256;

        contract C {
            struct S { uint256 $1field; }
            enum E { $2One, Two }

            S value;

            function read() public view returns (uint256) {
                return value.field.add(1);
            }

            function state() public pure returns (E) {
                return E.One;
            }
        }
        "#,
        "/Members.sol",
    );

    fixture.check_renames(
        &[("$1", "renamed"), ("$2", "Ready"), ("$3", "Math"), ("$4", "plus")],
        str![[r#"
$1:
/Members.sol:7:23-7:28 -> renamed
/Members.sol:11:21-11:26 -> renamed
$2:
/Members.sol:8:13-8:16 -> Ready
/Members.sol:14:17-14:20 -> Ready
$3:
/Members.sol:0:8-0:11 -> Math
/Members.sol:5:7-5:10 -> Math
$4:
/Members.sol:1:13-1:16 -> plus
/Members.sol:5:11-5:14 -> plus
/Members.sol:11:27-11:30 -> plus

"#]],
    );
}

#[test]
fn rejects_targets_with_ambiguous_references() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Ambiguous.sol
        contract C {
            function $1pick(uint8 value) internal pure returns (uint8) {
                return value;
            }

            function $2pick(uint256 value) internal pure returns (uint256) {
                return value;
            }

            function call(uint8 value) public pure returns (uint256) {
                return $3pick(value);
            }
        }
        "#,
        "/Ambiguous.sol",
    );

    for marker in ["$1", "$2", "$3"] {
        fixture.check_prepare_rename(marker, "<none>\n");
    }
    fixture.check_rename("$1", "renamed", "<none>\n");
}

#[test]
fn resolves_overloads_and_shadowed_names_before_renaming() {
    let fixture = RequestFixture::new(
        r#"
        //- /Resolution.sol
        contract C {
            function $1pick(uint256 value) internal pure returns (uint256) {
                return value;
            }
            function $2pick(bytes32 value) internal pure returns (bytes32) {
                return value;
            }
            function call(uint256 value) public pure returns (uint256) {
                return $3pick(value);
            }
            uint256 $4value;
            function read(uint256 $5value) public pure returns (uint256) {
                return $6value;
            }
            function state() public view returns (uint256) {
                return value;
            }
        }
        "#,
        "/Resolution.sol",
    );

    fixture.check_prepare_rename("$3", "8:15-8:19\n");
    fixture.check_renames(
        &[("$1", "selected"), ("$2", "other"), ("$4", "stateValue"), ("$5 $6", "localValue")],
        str![[r#"
$1:
/Resolution.sol:1:13-1:17 -> selected
/Resolution.sol:8:15-8:19 -> selected
$2:
/Resolution.sol:4:13-4:17 -> other
$4:
/Resolution.sol:10:12-10:17 -> stateValue
/Resolution.sol:15:15-15:20 -> stateValue
$5 $6:
/Resolution.sol:11:26-11:31 -> localValue
/Resolution.sol:12:15-12:20 -> localValue

"#]],
    );
}

#[test]
fn rejects_stale_disk_and_vfs_contents() {
    // The declaration range still matches, but every analyzed file must be unchanged.
    let disk = RequestFixture::new("//- /Disk.sol\ncontract C { uint256 $1value; }\n", "/Disk.sol");
    disk.write_file("/Disk.sol", "contract C { uint256 value; uint256 other; }\n");
    disk.check_rename_error("$1", "renamed", ErrorCode::CONTENT_MODIFIED);

    let mut open =
        RequestFixture::new("//- /Open.sol open\ncontract C { uint256 $1value; }\n", "/Open.sol");
    open.set_open_file_contents("/Open.sol", "contract C { uint256 changed; }");
    open.check_rename_error("$1", "renamed", ErrorCode::CONTENT_MODIFIED);
}

#[test]
fn in_flight_rename_response_keeps_the_validated_version() {
    let fixture =
        RequestFixture::new("//- /Race.sol open\ncontract C { uint256 $1value; }\n", "/Race.sol");
    let contents = fixture.project_contents("/Race.sol");
    let (mut state, params) = fixture.rename_state_and_params("$1", "renamed");
    let uri = params.text_document_position.text_document.uri.clone();
    let path = crate::proto::vfs_path(&uri).unwrap();

    let capabilities = json!({ "workspace": { "workspaceEdit": { "documentChanges": true } } });
    let initialize = with_capabilities(fixture.project().initialize_params(), capabilities);
    state.config = Arc::new(negotiate_capabilities(initialize).1);

    change(&mut state, &uri, 7, contents.as_str());
    assert_eq!(state.vfs.read().get_file_version(&path), Some(7));

    let runtime = single_blocking_worker_runtime();
    let _entered = runtime.enter();
    let rename = crate::handlers::rename(&mut state, params);
    let vfs = Arc::clone(&state.vfs);
    let vfs_guard = vfs.write();
    let rename = start_request(rename);
    drop(vfs_guard);
    // The only blocking worker runs this after the rename validation task completes.
    runtime.block_on(within("rename validation", tokio::task::spawn_blocking(|| {}))).unwrap();

    let changed_contents = format!("// changed while rename was in flight\n{contents}");
    change(&mut state, &uri, 8, changed_contents);
    assert_eq!(state.vfs.read().get_file_version(&path), Some(8));

    let edit = expect_ready(rename).unwrap().unwrap();
    assert!(edit.changes.is_none());
    let Some(DocumentChanges::Edits(edits)) = edit.document_changes else {
        panic!("expected versioned document edits");
    };
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].text_document.uri, uri);
    assert_eq!(edits[0].text_document.version, Some(7));
}

#[test]
fn validates_and_edits_utf16_ranges() {
    let fixture = RequestFixture::new(
        r#"
        //- /Utf16.sol open
        contract C {
            string constant TEXT = unicode"中文😀"; uint256 $1value;

            function read() public view returns (uint256) {
                string memory ignored = unicode"😀"; return value;
            }
        }
        "#,
        "/Utf16.sol",
    );

    fixture.check_rename(
        "$1",
        "renamed",
        str![[r#"
/Utf16.sol:1:50-1:55 -> renamed
/Utf16.sol:3:52-3:57 -> renamed

"#]],
    );
}

#[test]
fn remaps_and_unifies_rename_ids_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Lib.sol
        contract Target {}

        //- /Shared.sol
        import "./Lib.sol" as $5Lib;
        contract $6Shared {
            Lib.Target value;
        }

        //- /first/Lib.sol
        contract FirstTarget {}

        //- /first/Main.sol
        import "./Lib.sol" as FirstNS;
        import "../Shared.sol";

        contract First is Shared {
            FirstNS.FirstTarget first;
        }

        //- /second/Lib.sol
        contract $1SecondTarget {}

        //- /second/Main.sol
        import "./Lib.sol" as $2SecondNS;
        import {$3SecondTarget as $4Alias} from "./Lib.sol";
        import "../Shared.sol";

        contract Second is Shared {
            SecondNS.SecondTarget direct;
            Alias aliased;
        }
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_renames(
        &[
            ("$1", "Renamed"),
            ("$2", "Renamed"),
            ("$4", "Renamed"),
            ("$5", "Renamed"),
            ("$6", "Renamed"),
        ],
        str![[r#"
$1:
/second/Lib.sol:0:9-0:21 -> Renamed
/second/Main.sol:1:8-1:20 -> Renamed
/second/Main.sol:4:13-4:25 -> Renamed
$2:
/second/Main.sol:0:22-0:30 -> Renamed
/second/Main.sol:4:4-4:12 -> Renamed
$4:
/second/Main.sol:1:24-1:29 -> Renamed
/second/Main.sol:5:4-5:9 -> Renamed
$5:
/Shared.sol:0:22-0:25 -> Renamed
/Shared.sol:2:4-2:7 -> Renamed
$6:
/Shared.sol:1:9-1:15 -> Renamed
/first/Main.sol:2:18-2:24 -> Renamed
/second/Main.sol:3:19-3:25 -> Renamed

"#]],
    );

    let state = fixture.state();
    let (uri, position) = fixture.marker_location("$6");
    let tables = state.symbol_tables.load();
    let candidate = tables.rename_candidate(&uri, position).unwrap();
    // Validation groups adjacent locations by file, so preserve unique URI/range order.
    assert_eq!(candidate.locations.len(), 3);
    assert_eq!(candidate.analyzed_contents.len(), 3);
    assert!(candidate.locations.is_sorted_by(|a, b| {
        (&a.uri, crate::proto::range_key(a.range)) < (&b.uri, crate::proto::range_key(b.range))
    }));
}

#[test]
fn preserves_import_aliases_in_declaration_free_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Empty.sol
        pragma solidity ^0.8.0;

        //- /Main.sol
        import "./Empty.sol" as $1Alias;
        "#,
        &["/Main.sol"],
    );

    fixture.check_rename("$1", "Renamed", "/Main.sol:0:24-0:29 -> Renamed\n");
}

#[test]
fn unifies_override_families_across_analysis_batches() {
    let source = r#"
        //- /Base.sol
        contract Base {
            function $1run() public virtual {}
        }
        //- /Left.sol
        import "./Base.sol";
        contract Left is Base {
            function $2run() public override {}
            function call() public { run(); }
        }
        //- /Right.sol
        import "./Base.sol";
        contract Right is Base {
            function $3run() public override {}
            function call() public { run(); }
        }
    "#;
    for paths in [["/Left.sol", "/Right.sol"], ["/Right.sol", "/Left.sol"]] {
        let fixture = RequestFixture::new_in_batches(source, &paths);
        for (marker, range) in [("$1", "1:13-1:16\n"), ("$2", "2:13-2:16\n"), ("$3", "2:13-2:16\n")]
        {
            fixture.check_prepare_rename(marker, range);
        }
        fixture.check_renames(
            &[("$1 $2 $3", "renamed")],
            str![[r#"
$1 $2 $3:
/Base.sol:1:13-1:16 -> renamed
/Left.sol:2:13-2:16 -> renamed
/Left.sol:3:29-3:32 -> renamed
/Right.sol:2:13-2:16 -> renamed
/Right.sol:3:29-3:32 -> renamed

"#]],
        );
    }
}

#[test]
fn rejects_conflicting_source_snapshots_across_analysis_batches() {
    let source = r#"
        //- /Shared.sol open
        contract C {
            uint256 $1value;
            // The saved file still has a code reference.
            //         value
        }

        //- /first/Main.sol
        import "../Shared.sol";
        contract First { C value; }
        "#;
    let disk_contents = r#"contract C {
    uint256 value;
    function read() public view returns (uint256) {
        return value;
    }
}
"#;

    for paths in [["/first/Main.sol", "/Shared.sol"], ["/Shared.sol", "/first/Main.sol"]] {
        let fixture = RequestFixture::new_in_batches_with_stale_disk(
            source,
            "/Shared.sol",
            disk_contents,
            &paths,
        );
        fixture.check_rename_error("$1", "renamed", ErrorCode::CONTENT_MODIFIED);
    }
}

/// Publishes the current config as the analysis config, marked incomplete unless `complete`.
fn publish_analysis_config(state: &GlobalState, complete: bool) {
    assert!(!state.config.may_omit_source_files());
    let mut config = (*state.config).clone();
    if !complete {
        config.mark_analysis_source_files_incomplete();
    }
    state.analysis_commit.lock().analysis_config = Some(Arc::new(config));
}

/// Renders the rename edits, or the request error that prepare-rename must share.
async fn rename_report(state: &mut GlobalState, params: RenameParams, root: &Path) -> String {
    let prepare = crate::handlers::prepare_rename(state, params.text_document_position.clone());
    let prepared = within("prepare rename", prepare).await;
    match (prepared, crate::handlers::rename(state, params).await) {
        (Ok(Some(_)), Ok(edit)) => rename_output(root, edit),
        (Err(prepared), Err(error)) => {
            assert_eq!(
                (prepared.code, error.code),
                (ErrorCode::REQUEST_FAILED, ErrorCode::REQUEST_FAILED)
            );
            assert_eq!(prepared.message, error.message);
            format!("{}\n", error.message)
        }
        other => panic!("prepare and rename disagree: {other:?}"),
    }
}
