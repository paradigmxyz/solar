use super::support::RequestFixture;
use crate::{config::negotiate_capabilities, global_state::GlobalState};
use async_lsp::ErrorCode;
use lsp_types::{
    DidChangeTextDocumentParams, DocumentChanges, InitializeParams, TextDocumentContentChangeEvent,
    Url, VersionedTextDocumentIdentifier, WorkspaceClientCapabilities,
    WorkspaceEditClientCapabilities,
};
use snapbox::str;
use std::{
    future::Future,
    sync::{Arc, mpsc},
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

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

#[test]
fn renames_named_modifier_and_base_constructor_arguments() {
    let fixture = RequestFixture::new(
        r#"
        //- /NamedModifiers.sol
        contract Base {
            constructor(uint256 $3amount) {}
        }

        contract Child is Base({$4amount: 1}) {
            modifier guarded(uint256 $1amount) { _; }
            function run() public guarded({$2amount: 1}) {}
        }
        "#,
        "/NamedModifiers.sol",
    );

    let modifier = str![[r#"
/NamedModifiers.sol:4:29-4:35 -> value
/NamedModifiers.sol:5:35-5:41 -> value

"#]];
    fixture.check_rename("$1", "value", modifier.clone());
    fixture.check_rename("$2", "value", modifier);

    let constructor = str![[r#"
/NamedModifiers.sol:1:24-1:30 -> value
/NamedModifiers.sol:3:24-3:30 -> value

"#]];
    fixture.check_rename("$3", "value", constructor.clone());
    fixture.check_rename("$4", "value", constructor);
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

    fixture.check_rename(
        "$2",
        "account",
        str![[r#"
/MappingNames.sol:1:20-1:25 -> account
/MappingNames.sol:3:30-3:35 -> account

"#]],
    );
    fixture.check_rename(
        "$4",
        "delegate",
        str![[r#"
/MappingNames.sol:1:45-1:52 -> delegate
/MappingNames.sol:3:49-3:56 -> delegate

"#]],
    );
    fixture.check_prepare_rename("$5", "1:64-1:71\n");
    fixture.check_rename(
        "$5",
        "amount",
        str![[r#"
/MappingNames.sol:1:64-1:71 -> amount

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
fn renames_inherited_qualified_type_components() {
    let fixture = RequestFixture::new(
        r#"
        //- /InheritedQualifier.sol
        contract Base { struct S { uint256 field; } }
        contract $1Child is Base {}
        contract Use { $2Child.S value; }
        "#,
        "/InheritedQualifier.sol",
    );

    let expected = str![[r#"
/InheritedQualifier.sol:1:9-1:14 -> Renamed
/InheritedQualifier.sol:2:15-2:20 -> Renamed

"#]];
    fixture.check_rename("$1", "Renamed", expected.clone());
    fixture.check_rename("$2", "Renamed", expected);
}

#[test]
fn renames_storage_layout_expressions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Layout.sol
        uint256 constant $1BASE = 42;
        contract C layout at BASE {}
        "#,
        "/Layout.sol",
    );

    fixture.check_rename(
        "$1",
        "RENAMED",
        str![[r#"
/Layout.sol:0:17-0:21 -> RENAMED
/Layout.sol:1:21-1:25 -> RENAMED

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
            fallback() external virtual {}
            receive() external payable virtual {}
        }

        contract Child is Base {
            function f() public override(Base) {}
            fallback() external override(Base) {}
            receive() external payable override(Base) {}
        }
        "#,
        "/Override.sol",
    );

    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Override.sol:0:9-0:13 -> Renamed
/Override.sol:5:18-5:22 -> Renamed
/Override.sol:6:33-6:37 -> Renamed
/Override.sol:7:33-7:37 -> Renamed
/Override.sol:8:40-8:44 -> Renamed

"#]],
    );
}

#[test]
fn renames_validated_natspec_parameter_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /NatSpec.sol
        contract C {
            /// @param amount Payment amount.
            function pay(uint256 $1amount) public {}
        }
        "#,
        "/NatSpec.sol",
    );

    fixture.check_rename(
        "$1",
        "value",
        str![[r#"
/NatSpec.sol:1:15-1:21 -> value
/NatSpec.sol:2:25-2:31 -> value

"#]],
    );
}

#[test]
fn renames_validated_natspec_return_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /NatSpecReturn.sol
        contract C {
            /// @return result The value.
            function f() public pure returns (uint256 $1result) {
                result = 1;
            }
        }
        "#,
        "/NatSpecReturn.sol",
    );

    fixture.check_rename(
        "$1",
        "value",
        str![[r#"
/NatSpecReturn.sol:1:16-1:22 -> value
/NatSpecReturn.sol:2:46-2:52 -> value
/NatSpecReturn.sol:3:8-3:14 -> value

"#]],
    );
}

#[test]
fn renames_validated_natspec_inheritdoc_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Inheritdoc.sol
        contract $1Base {
            function run() public virtual {}
        }

        contract Child is Base {
            /// @inheritdoc Base
            function run() public override(Base) {}
        }
        "#,
        "/Inheritdoc.sol",
    );

    fixture.check_rename(
        "$1",
        "Parent",
        str![[r#"
/Inheritdoc.sol:0:9-0:13 -> Parent
/Inheritdoc.sol:3:18-3:22 -> Parent
/Inheritdoc.sol:4:20-4:24 -> Parent
/Inheritdoc.sol:5:35-5:39 -> Parent

"#]],
    );
}

#[test]
fn renames_override_families_from_base_and_derived_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /OverrideFamily.sol
        contract Base {
            function $1run() public virtual {}
            modifier $3guard() virtual { _; }
        }

        contract Child is Base {
            function $2run() public override {}
            modifier $4guard() override { _; }
            function call() public $5guard { $6run(); }
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

    let functions = str![[r#"
/OverrideFamily.sol:1:13-1:16 -> renamed
/OverrideFamily.sol:5:13-5:16 -> renamed
/OverrideFamily.sol:7:35-7:38 -> renamed

"#]];
    fixture.check_rename("$1", "renamed", functions.clone());
    fixture.check_rename("$2", "renamed", functions);

    let modifiers = str![[r#"
/OverrideFamily.sol:2:13-2:18 -> checked
/OverrideFamily.sol:6:13-6:18 -> checked
/OverrideFamily.sol:7:27-7:32 -> checked

"#]];
    fixture.check_rename("$3", "checked", modifiers.clone());
    fixture.check_rename("$4", "checked", modifiers);

    let getter = str![[r#"
/OverrideFamily.sol:10:13-10:18 -> amount
/OverrideFamily.sol:13:28-13:33 -> amount
/OverrideFamily.sol:14:66-14:71 -> amount

"#]];
    fixture.check_rename("$7", "amount", getter.clone());
    fixture.check_rename("$8", "amount", getter);
}

#[test]
fn does_not_merge_function_typed_parameters_into_override_families() {
    let fixture = RequestFixture::new(
        r#"
        //- /Callback.sol
        contract Base {
            function $3hook(uint256 x) public virtual returns (uint256) { return x; }
        }

        contract Child is Base {
            function use(function(uint256) external returns (uint256) $1hook, uint256 x)
                public returns (uint256) { return $2hook(x); }
        }
        "#,
        "/Callback.sol",
    );

    fixture.check_rename(
        "$1",
        "callback",
        str![[r#"
/Callback.sol:4:62-4:66 -> callback
/Callback.sol:5:42-5:46 -> callback

"#]],
    );
    fixture.check_rename(
        "$3",
        "renamed",
        str![[r#"
/Callback.sol:1:13-1:17 -> renamed

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

    let disk_with_matching_range = RequestFixture::new(
        r#"
        //- /MatchingRange.sol
        contract C {
            uint256 $1value;
        }
        "#,
        "/MatchingRange.sol",
    );
    disk_with_matching_range.write_file(
        "/MatchingRange.sol",
        r#"contract C {
    uint256 value;
    function read() public view returns (uint256) {
        return value;
    }
}
"#,
    );
    disk_with_matching_range.check_rename_error("$1", "renamed", ErrorCode::CONTENT_MODIFIED);
}

#[test]
fn in_flight_rename_response_keeps_the_validated_version() {
    let fixture = RequestFixture::new(
        r#"
        //- /Race.sol open
        contract C { uint256 $1value; }
        "#,
        "/Race.sol",
    );
    let contents = fixture.project_contents("/Race.sol");
    let (mut state, params) = fixture.rename_state_and_params("$1", "renamed");
    let uri = params.text_document_position.text_document.uri.clone();
    let path = crate::proto::vfs_path(&uri).unwrap();

    let mut initialize = InitializeParams::default();
    initialize.capabilities.workspace = Some(WorkspaceClientCapabilities {
        workspace_edit: Some(WorkspaceEditClientCapabilities {
            document_changes: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(initialize);
    state.config = Arc::new(config);
    assert!(state.config.supports_workspace_edit_document_changes());

    set_document_contents(&mut state, uri.clone(), 7, &contents);
    assert_eq!(state.vfs.read().get_file_version(&path), Some(7));

    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let entered = runtime.enter();
    let mut rename = Box::pin(crate::handlers::rename(&mut state, params));
    let vfs = Arc::clone(&state.vfs);
    let vfs_guard = vfs.write();
    let (wake_tx, wake_rx) = mpsc::channel();
    let waker = Waker::from(Arc::new(CompletionWaker(wake_tx)));
    let mut context = Context::from_waker(&waker);

    assert!(rename.as_mut().poll(&mut context).is_pending());
    assert_eq!(wake_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
    drop(vfs_guard);
    wake_rx.recv_timeout(Duration::from_secs(5)).expect("rename validation task should complete");

    let changed_contents = format!("// changed while rename was in flight\n{contents}");
    set_document_contents(&mut state, uri.clone(), 8, &changed_contents);
    assert_eq!(state.vfs.read().get_file_version(&path), Some(8));

    let Poll::Ready(response) = rename.as_mut().poll(&mut context) else {
        panic!("completed rename task should make the handler ready");
    };
    let edit = response.unwrap().unwrap();
    assert!(edit.changes.is_none());
    let Some(DocumentChanges::Edits(edits)) = edit.document_changes else {
        panic!("expected versioned document edits");
    };
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].text_document.uri, uri);
    assert_eq!(edits[0].text_document.version, Some(7));

    drop(rename);
    drop(entered);
    drop(runtime);
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

    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Main.sol:0:24-0:29 -> Renamed

"#]],
    );
}

#[test]
fn unifies_shared_declarations_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Shared.sol
        contract $1Shared {}

        //- /first/Main.sol
        import "../Shared.sol";
        contract First is Shared {}

        //- /second/Main.sol
        import "../Shared.sol";
        contract Second is Shared {}
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Shared.sol:0:9-0:15 -> Renamed
/first/Main.sol:1:18-1:24 -> Renamed
/second/Main.sol:1:19-1:25 -> Renamed

"#]],
    );
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

#[test]
fn unifies_shared_import_aliases_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Lib.sol
        contract Target {}

        //- /Shared.sol
        import "./Lib.sol" as $1Lib;
        contract Shared {
            Lib.Target value;
        }

        //- /first/Main.sol
        import "../Shared.sol";

        //- /second/Main.sol
        import "../Shared.sol";
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Shared.sol:0:22-0:25 -> Renamed
/Shared.sol:2:4-2:7 -> Renamed

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
    fixture.check_rename_error("$1", "leave", ErrorCode::INVALID_PARAMS);
    fixture.check_rename_error("$2", "add", ErrorCode::INVALID_PARAMS);
}

struct CompletionWaker(mpsc::Sender<()>);

impl Wake for CompletionWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.0.send(());
    }
}

fn set_document_contents(state: &mut GlobalState, uri: Url, version: i32, text: &str) {
    let result = crate::handlers::did_change_text_document(
        state,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, version),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.into(),
            }],
        },
    );
    assert!(matches!(result, std::ops::ControlFlow::Continue(())));
}
