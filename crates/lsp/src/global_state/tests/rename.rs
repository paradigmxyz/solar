use super::support::RequestFixture;
use async_lsp::ErrorCode;
use snapbox::str;

#[test]
fn prepares_and_renames_a_state_variable() {
    let fixture = RequestFixture::new(
        r#"
        //- /Basic.sol
        contract C {
            uint256 $1value;

            function set(uint256 next) public {
                value = next;
            }

            function get() public view returns (uint256) {
                return value;
            }
        }
        "#,
        "/Basic.sol",
    );

    fixture.check_prepare_rename(
        "$1",
        str![[r#"
1:12-1:17

"#]],
    );
    fixture.check_rename(
        "$1",
        "renamed",
        str![[r#"
/Basic.sol:1:12-1:17 -> renamed
/Basic.sol:3:8-3:13 -> renamed
/Basic.sol:6:15-6:20 -> renamed

"#]],
    );
}

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
    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Variable.sol:1:15-1:23 -> Renamed
/Variable.sol:3:8-3:16 -> Renamed
/Variable.sol:4:20-4:28 -> Renamed
/Variable.sol:8:13-8:21 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$4",
        "Renamed",
        str![[r#"
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

    fixture.check_prepare_rename("$1", "0:27-0:39\n");
    fixture.check_prepare_rename("$3", "1:20-1:36\n");
    fixture.check_prepare_rename("$2", "1:8-1:16\n");
    fixture.check_prepare_rename("$6", "4:4-4:16\n");
    fixture.check_prepare_rename("$5", "3:4-3:20\n");
    fixture.check_prepare_rename("$7", "4:17-4:25\n");
    fixture.check_prepare_rename("$8", "5:4-5:8\n");
    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Main.sol:0:27-0:39 -> Renamed
/Main.sol:4:4-4:16 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$3",
        "Renamed",
        str![[r#"
/Main.sol:1:20-1:36 -> Renamed
/Main.sol:3:4-3:20 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$5",
        "Renamed",
        str![[r#"
/Main.sol:1:20-1:36 -> Renamed
/Main.sol:3:4-3:20 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/Imported.sol:0:9-0:17 -> Renamed
/Imported.sol:2:4-2:12 -> Renamed
/Main.sol:1:8-1:16 -> Renamed
/Main.sol:4:17-4:25 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$4",
        "Renamed",
        str![[r#"
/Imported.sol:1:9-1:13 -> Renamed
/Main.sol:1:38-1:42 -> Renamed
/Main.sol:5:4-5:8 -> Renamed

"#]],
    );
}

#[test]
fn validates_new_names_and_handles_noop_renames() {
    let fixture = RequestFixture::new(
        r#"
        //- /Names.sol
        contract C {
            uint256 $1value;
        }
        "#,
        "/Names.sol",
    );

    fixture.check_rename_error("$1", "not a name", ErrorCode::INVALID_PARAMS);
    fixture.check_rename_error("$1", "256value", ErrorCode::INVALID_PARAMS);
    fixture.check_rename_error("$1", "contract", ErrorCode::INVALID_PARAMS);
    fixture.check_rename_error("$1", "uint256", ErrorCode::INVALID_PARAMS);
    fixture.check_rename("$1", "value", "<none>\n");
}

#[test]
fn renames_qualified_type_components_and_bases() {
    let fixture = RequestFixture::new(
        r#"
        //- /Qualified.sol
        contract $1Outer {
            struct $2Inner { uint256 field; }
        }

        contract C is Outer {
            Outer.Inner value;

            function read() public view returns (Outer.Inner memory) {
                // Outer and Inner in this text must remain unchanged.
                return value;
            }
        }
        "#,
        "/Qualified.sol",
    );

    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Qualified.sol:0:9-0:14 -> Renamed
/Qualified.sol:3:14-3:19 -> Renamed
/Qualified.sol:4:4-4:9 -> Renamed
/Qualified.sol:5:41-5:46 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/Qualified.sol:1:11-1:16 -> Renamed
/Qualified.sol:4:10-4:15 -> Renamed
/Qualified.sol:5:47-5:52 -> Renamed

"#]],
    );
}

#[test]
fn renames_override_contract_paths() {
    let fixture = RequestFixture::new(
        r#"
        //- /Override.sol
        contract $1Base {
            function f() public virtual {}
        }

        contract Child is Base {
            function f() public override(Base) {}
        }
        "#,
        "/Override.sol",
    );

    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Override.sol:0:9-0:13 -> Renamed
/Override.sol:3:18-3:22 -> Renamed
/Override.sol:4:33-4:37 -> Renamed

"#]],
    );
}

#[test]
fn renames_modifier_declarations_and_uses() {
    let fixture = RequestFixture::new(
        r#"
        //- /Modifier.sol
        contract C {
            modifier $1guard(uint256 amount) {
                _;
            }
            function f() public $2guard(1) {}
        }
        "#,
        "/Modifier.sol",
    );

    fixture.check_prepare_rename("$2", "4:24-4:29\n");
    fixture.check_rename(
        "$2",
        "check",
        str![[r#"
/Modifier.sol:1:13-1:18 -> check
/Modifier.sol:4:24-4:29 -> check

"#]],
    );
}

#[test]
fn renames_struct_fields_and_enum_variants() {
    let fixture = RequestFixture::new(
        r#"
        //- /Members.sol
        contract C {
            struct $1S { uint256 $2field; }
            enum $3E { $4One, Two }

            S value;

            function read() public view returns (uint256) {
                return value.field;
            }

            function state() public pure returns (E) {
                return E.One;
            }
        }
        "#,
        "/Members.sol",
    );

    fixture.check_rename(
        "$2",
        "renamed",
        str![[r#"
/Members.sol:1:23-1:28 -> renamed
/Members.sol:5:21-5:26 -> renamed

"#]],
    );
    fixture.check_rename(
        "$4",
        "Ready",
        str![[r#"
/Members.sol:2:13-2:16 -> Ready
/Members.sol:8:17-8:20 -> Ready

"#]],
    );
}

#[test]
fn renames_using_paths_and_attached_calls() {
    let fixture = RequestFixture::new(
        r#"
        //- /Using.sol
        library $1Lib {
            function $2add(uint256 self, uint256 other) internal pure returns (uint256) {
                return self + other;
            }
        }

        using {Lib.add} for uint256;

        contract C {
            function sum(uint256 value) public pure returns (uint256) {
                return value.add(1);
            }
        }
        "#,
        "/Using.sol",
    );

    fixture.check_rename(
        "$1",
        "Math",
        str![[r#"
/Using.sol:0:8-0:11 -> Math
/Using.sol:5:7-5:10 -> Math

"#]],
    );
    fixture.check_rename(
        "$2",
        "plus",
        str![[r#"
/Using.sol:1:13-1:16 -> plus
/Using.sol:5:11-5:14 -> plus
/Using.sol:8:21-8:24 -> plus

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

    fixture.check_prepare_rename("$1", "<none>\n");
    fixture.check_prepare_rename("$2", "<none>\n");
    fixture.check_prepare_rename("$3", "<none>\n");
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
            function read(uint256 $5value) public view returns (uint256) {
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
    fixture.check_rename(
        "$1",
        "selected",
        str![[r#"
/Resolution.sol:1:13-1:17 -> selected
/Resolution.sol:8:15-8:19 -> selected

"#]],
    );
    fixture.check_rename(
        "$2",
        "other",
        str![[r#"
/Resolution.sol:4:13-4:17 -> other

"#]],
    );
    fixture.check_rename(
        "$4",
        "stateValue",
        str![[r#"
/Resolution.sol:10:12-10:17 -> stateValue
/Resolution.sol:15:15-15:20 -> stateValue

"#]],
    );
    fixture.check_rename(
        "$5",
        "localValue",
        str![[r#"
/Resolution.sol:11:26-11:31 -> localValue
/Resolution.sol:12:15-12:20 -> localValue

"#]],
    );
}

#[test]
fn rejects_stale_disk_and_vfs_contents() {
    let disk = RequestFixture::new(
        r#"
        //- /Disk.sol
        contract C { uint256 $1value; }
        "#,
        "/Disk.sol",
    );
    disk.write_file("/Disk.sol", "contract C { uint256 changed; }");
    disk.check_rename_error("$1", "renamed", ErrorCode::CONTENT_MODIFIED);

    let mut open = RequestFixture::new(
        r#"
        //- /Open.sol open
        contract C { uint256 $1value; }
        "#,
        "/Open.sol",
    );
    open.set_open_file_contents("/Open.sol", "contract C { uint256 changed; }");
    open.check_rename_error("$1", "renamed", ErrorCode::CONTENT_MODIFIED);
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
fn prepare_rename_rejects_keywords_builtins_and_whitespace() {
    let fixture = RequestFixture::new(
        r#"
        //- /InvalidPositions.sol
        $1contract C {
            uint256 value;

            $2constructor() {}

            function read() public view returns (uint256) {
                uint256 height = $3block.number;
                return $4  value;
            }
        }
        "#,
        "/InvalidPositions.sol",
    );

    fixture.check_prepare_rename("$1", "<none>\n");
    fixture.check_prepare_rename("$2", "<none>\n");
    fixture.check_prepare_rename("$3", "<none>\n");
    fixture.check_prepare_rename("$4", "<none>\n");
}

#[test]
fn remaps_rename_ids_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /first/Lib.sol
        contract FirstTarget {}

        //- /first/Main.sol
        import "./Lib.sol" as FirstNS;

        contract First {
            FirstNS.FirstTarget value;
        }

        //- /second/Lib.sol
        contract $1SecondTarget {}

        //- /second/Main.sol
        import "./Lib.sol" as $2SecondNS;
        import {$3SecondTarget as $4Alias} from "./Lib.sol";

        contract Second {
            SecondNS.SecondTarget direct;
            Alias aliased;
        }
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/second/Lib.sol:0:9-0:21 -> Renamed
/second/Main.sol:1:8-1:20 -> Renamed
/second/Main.sol:3:13-3:25 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/second/Main.sol:0:22-0:30 -> Renamed
/second/Main.sol:3:4-3:12 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$4",
        "Renamed",
        str![[r#"
/second/Main.sol:1:24-1:29 -> Renamed
/second/Main.sol:4:4-4:9 -> Renamed

"#]],
    );
}

#[test]
fn renames_solidity_variables_but_not_yul_locals_in_inline_assembly() {
    let fixture = RequestFixture::new(
        r#"
        //- /Assembly.sol
        contract C {
            uint256 $1stored;

            function run(uint256 $2input) public returns (uint256 output) {
                assembly {
                    let $3local := input
                    sstore(stored.slot, add(local, input))
                    output := local
                }
            }
        }
        "#,
        "/Assembly.sol",
    );

    fixture.check_rename(
        "$1",
        "renamed",
        str![[r#"
/Assembly.sol:1:12-1:18 -> renamed
/Assembly.sol:5:19-5:25 -> renamed

"#]],
    );
    fixture.check_rename(
        "$2",
        "renamed",
        str![[r#"
/Assembly.sol:2:25-2:30 -> renamed
/Assembly.sol:4:25-4:30 -> renamed
/Assembly.sol:5:43-5:48 -> renamed

"#]],
    );
    fixture.check_prepare_rename("$3", "<none>\n");
    fixture.check_rename("$3", "renamed", "<none>\n");
}
