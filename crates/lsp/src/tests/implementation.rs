use super::support::RequestFixture;
use snapbox::str;

#[test]
fn resolves_interface_function_to_concrete_implementations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Implementation.sol
        interface I {
            function $1run() external;
        }

        contract First is I {
            function $2run() external override {}
        }

        contract Second is I {
            function $3run() external override {}
        }
        "#,
        "/Implementation.sol",
    );

    let expected = str![[r#"
/Implementation.sol:4:13 function run() external override {}
/Implementation.sol:7:13 function run() external override {}

"#]];
    fixture.check_goto_implementation("$1", expected);
    fixture.check_goto_implementation("$2", "<none>\n");
    fixture.check_goto_implementation("$3", "<none>\n");
}

#[test]
fn filters_abstract_override_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /AbstractImplementation.sol
        abstract contract Base {
            function $1run() external virtual;
        }

        abstract contract Middle is Base {
            function $2run() external override virtual;
        }

        contract Concrete is Middle {
            function $3run() external override {}
        }

        abstract contract Unimplemented {
            function $4missing() external virtual;
        }
        "#,
        "/AbstractImplementation.sol",
    );

    let expected = str![[r#"
/AbstractImplementation.sol:7:13 function run() external override {}

"#]];
    fixture.check_goto_implementation("$1", expected.clone());
    fixture.check_goto_implementation("$2", expected);
    fixture.check_goto_implementation("$3", "<none>\n");
    fixture.check_goto_implementation("$4", "<none>\n");
}

#[test]
fn traverses_multilevel_and_multiple_inheritance() {
    let fixture = RequestFixture::new(
        r#"
        //- /Inheritance.sol
        interface Root {
            function $1run() external;
        }

        contract Base is Root {
            function $2run() public virtual {}
        }

        contract Middle is Base {}

        contract Leaf is Middle {
            function $3run() public override {}
        }

        interface Left {
            function $4ping() external;
        }

        interface Right {
            function $5ping() external;
        }

        contract Both is Left, Right {
            function $6ping() external override(Left, Right) {}
        }
        "#,
        "/Inheritance.sol",
    );

    let root_run = str![[r#"
/Inheritance.sol:4:13 function run() public virtual {}
/Inheritance.sol:8:13 function run() public override {}

"#]];
    fixture.check_goto_implementation("$1", root_run);
    fixture.check_goto_implementation(
        "$2",
        str![[r#"
/Inheritance.sol:8:13 function run() public override {}

"#]],
    );
    fixture.check_goto_implementation("$3", "<none>\n");

    let ping = str![[r#"
/Inheritance.sol:17:13 function ping() external override(Left, Right) {}

"#]];
    fixture.check_goto_implementation("$4", ping.clone());
    fixture.check_goto_implementation("$5", ping);
    fixture.check_goto_implementation("$6", "<none>\n");
}

#[test]
fn excludes_ancestors_and_self_from_transitive_implementations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Directional.sol
        contract Base {
            function $1run() public virtual {}
        }

        contract Middle is Base {
            function $2run() public virtual override {}
        }

        contract Leaf is Middle {
            function $3run() public override {}
        }

        contract Unoverridden {
            function $4standalone() public virtual {}
        }
        "#,
        "/Directional.sol",
    );

    fixture.check_goto_implementation(
        "$1",
        str![[r#"
/Directional.sol:4:13 function run() public virtual override {}
/Directional.sol:7:13 function run() public override {}

"#]],
    );
    fixture.check_goto_implementation(
        "$2",
        str![[r#"
/Directional.sol:7:13 function run() public override {}

"#]],
    );
    fixture.check_goto_implementation("$3", "<none>\n");
    fixture.check_goto_implementation("$4", "<none>\n");
}

#[test]
fn keeps_overloads_separate_and_resolves_call_sites() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overloads.sol
        interface I {
            function $1pick(uint256 value) external;
            function $2pick(string calldata value) external;
        }

        contract C is I {
            function $3pick(uint256 value) public override {}
            function $4pick(string calldata value) public override {}

            function call() public {
                $5pick(uint256(1));
            }
        }
        "#,
        "/Overloads.sol",
    );

    let integer = str![[r#"
/Overloads.sol:5:13 function pick(uint256 value) public override {}

"#]];
    fixture.check_goto_implementation("$1", integer.clone());
    fixture.check_goto_implementation("$3", "<none>\n");
    fixture.check_goto_implementation("$5", integer);

    let string = str![[r#"
/Overloads.sol:6:13 function pick(string calldata value) public override {}

"#]];
    fixture.check_goto_implementation("$2", string);
    fixture.check_goto_implementation("$4", "<none>\n");
}

#[test]
fn returns_standalone_concrete_functions_from_declarations_and_calls() {
    let fixture = RequestFixture::new(
        r#"
        //- /Standalone.sol
        contract C {
            function $1target() public {}

            function call() public {
                $2target();
            }
        }
        "#,
        "/Standalone.sol",
    );

    let expected = str![[r#"
/Standalone.sol:1:13 function target() public {}

"#]];
    fixture.check_goto_implementation("$1", expected.clone());
    fixture.check_goto_implementation("$2", expected);
}

#[test]
fn resolves_public_getter_overrides() {
    let fixture = RequestFixture::new(
        r#"
        //- /Getter.sol
        abstract contract GetterBase {
            function $1value() external view virtual returns (uint256);
        }

        contract GetterChild is GetterBase {
            uint256 public override $2value;

            function read() external view returns (uint256) {
                return this.$3value();
            }
        }
        "#,
        "/Getter.sol",
    );

    let expected = str![[r#"
/Getter.sol:4:28 uint256 public override value;

"#]];
    fixture.check_goto_implementation("$1", expected.clone());
    fixture.check_goto_implementation("$2", "<none>\n");
    fixture.check_goto_implementation("$3", expected);
}

#[test]
fn resolves_modifier_implementations_directionally() {
    let fixture = RequestFixture::new(
        r#"
        //- /Modifiers.sol
        contract Base {
            modifier $1guard() virtual { _; }
        }

        contract Child is Base {
            modifier $2guard() override { _; }
            function run() public $3guard {}
        }
        "#,
        "/Modifiers.sol",
    );

    let implementation = str![[r#"
/Modifiers.sol:4:13 modifier guard() override { _; }

"#]];
    fixture.check_goto_implementation("$1", implementation.clone());
    fixture.check_goto_implementation("$2", "<none>\n");
    fixture.check_goto_implementation("$3", implementation);
}

#[test]
fn resolves_named_import_aliases_but_not_namespace_aliases() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract $1Base {}

        //- /Main.sol
        import {Base as $2Alias} from "./Base.sol";
        import "./Base.sol" as $4NS;

        contract UsesAlias {
            $3Alias value;
            $5NS.Base other;
        }
        "#,
        "/Main.sol",
    );

    let expected = str![[r#"
/Base.sol:0:9 contract Base {}

"#]];
    fixture.check_goto_implementation("$1", expected.clone());
    fixture.check_goto_implementation("$2", expected.clone());
    fixture.check_goto_implementation("$3", expected);
    fixture.check_goto_implementation("$4", "<none>\n");
    fixture.check_goto_implementation("$5", "<none>\n");
}

#[test]
fn returns_singleton_locations_for_non_override_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Singletons.sol
        contract $1Container {
            struct $2Data { uint256 value; }
            enum $3Choice { None, Some }
            event $4Changed(uint256 value);
            error $5Failure(uint256 value);
        }
        "#,
        "/Singletons.sol",
    );

    fixture.check_goto_implementation(
        "$1",
        str![[r#"
/Singletons.sol:0:9 contract Container {

"#]],
    );
    fixture.check_goto_implementation(
        "$2",
        str![[r#"
/Singletons.sol:1:11 struct Data { uint256 value; }

"#]],
    );
    fixture.check_goto_implementation(
        "$3",
        str![[r#"
/Singletons.sol:2:9 enum Choice { None, Some }

"#]],
    );
    fixture.check_goto_implementation(
        "$4",
        str![[r#"
/Singletons.sol:3:10 event Changed(uint256 value);

"#]],
    );
    fixture.check_goto_implementation(
        "$5",
        str![[r#"
/Singletons.sol:4:10 error Failure(uint256 value);

"#]],
    );
}

#[test]
fn merges_override_descendants_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Base.sol
        interface I {
            function $1run() external;
        }

        //- /First.sol
        import "./Base.sol";
        contract First is I {
            function $2run() external override {}
        }

        //- /Second.sol
        import "./Base.sol";
        contract Second is I {
            function $3run() external override {}
        }
        "#,
        &["/First.sol", "/Second.sol"],
    );

    let expected = str![[r#"
/First.sol:2:13 function run() external override {}
/Second.sol:2:13 function run() external override {}

"#]];
    fixture.check_goto_implementation("$1", expected);
    fixture.check_goto_implementation("$2", "<none>\n");
    fixture.check_goto_implementation("$3", "<none>\n");
}

#[test]
fn merges_getter_override_descendants_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Base.sol
        abstract contract Base {
            function $1value() external view virtual returns (uint256);
        }

        //- /First.sol
        import "./Base.sol";
        contract First is Base {
            uint256 public override $2value;
        }

        //- /Second.sol
        import "./Base.sol";
        contract Second is Base {
            uint256 public override $3value;
        }
        "#,
        &["/First.sol", "/Second.sol"],
    );

    let expected = str![[r#"
/First.sol:2:28 uint256 public override value;
/Second.sol:2:28 uint256 public override value;

"#]];
    fixture.check_goto_implementation("$1", expected);
    fixture.check_goto_implementation("$2", "<none>\n");
    fixture.check_goto_implementation("$3", "<none>\n");
}

#[test]
fn remaps_named_alias_targets_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /first/Base.sol
        contract FirstBase {}

        //- /first/Main.sol
        import {FirstBase as FirstAlias} from "./Base.sol";
        contract First { FirstAlias value; }

        //- /second/Base.sol
        contract $1SecondBase {}

        //- /second/Main.sol
        import {SecondBase as $2Alias} from "./Base.sol";
        contract Second { $3Alias value; }
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    let expected = str![[r#"
/second/Base.sol:0:9 contract SecondBase {}

"#]];
    fixture.check_goto_implementation("$1", expected.clone());
    fixture.check_goto_implementation("$2", expected.clone());
    fixture.check_goto_implementation("$3", expected);
}

#[test]
fn ignores_conflicting_source_snapshots_across_analysis_batches() {
    let source = r#"
        //- /Shared.sol open
        abstract contract Base {
            function $1bravo() external virtual;
        }

        //- /first/Main.sol
        import "../Shared.sol";
        contract Impl is Base {
            function alpha() external override {}
        }
        "#;
    let disk_contents = r#"abstract contract Base {
    function alpha() external virtual;
}
"#;

    for paths in [["/first/Main.sol", "/Shared.sol"], ["/Shared.sol", "/first/Main.sol"]] {
        let fixture = RequestFixture::new_in_batches_with_stale_disk(
            source,
            "/Shared.sol",
            disk_contents,
            &paths,
        );
        fixture.check_goto_implementation("$1", "<none>\n");
    }
}

#[test]
fn isolates_conflicting_dependency_implementations_across_analysis_batches() {
    let source = r#"
        //- /Shared.sol open
        import "./open/OpenImpl.sol";
        abstract contract Base {
            function run(uint256) public virtual {}
        }

        //- /open/OpenImpl.sol
        import "../Shared.sol";
        contract OpenImpl is Base {
            function run(uint256) public override {}
        }

        //- /first/Main.sol
        import "../Shared.sol";

        contract DiskImpl is Base {
            function run(bytes32) public override {}
        }

        contract UsesBase {
            function call(Base base) public {
                base.$1run(bytes32(0));
            }
        }
        "#;
    let disk_contents = r#"// Different snapshot of the same dependency.
abstract contract Base {
    function run(bytes32) public virtual;
}

contract SharedImpl is Base {
    function run(bytes32) public override {}
}
"#;

    for paths in [["/first/Main.sol", "/Shared.sol"], ["/Shared.sol", "/first/Main.sol"]] {
        let fixture = RequestFixture::new_in_batches_with_stale_disk(
            source,
            "/Shared.sol",
            disk_contents,
            &paths,
        );
        fixture.check_goto_implementation(
            "$1",
            str![[r#"
/first/Main.sol:2:13 function run(bytes32) public override {}

"#]],
        );
    }
}
