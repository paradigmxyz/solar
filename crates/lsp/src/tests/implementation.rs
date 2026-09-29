use super::support::{Query, RequestFixture};
use snapbox::str;

#[test]
fn resolves_implementations_directionally() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract $36Aliased {}

        //- /Main.sol
        import {Aliased as $37Alias} from "./Base.sol";
        import "./Base.sol" as $38NS;

        interface I {
            function $1run() external;
        }
        contract First is I {
            function $2run() external override {}
        }
        contract Second is I {
            function $3run() external override {}
        }

        abstract contract AbstractBase {
            function $4run() external virtual;
        }
        abstract contract AbstractMiddle is AbstractBase {
            function $5run() external override virtual;
        }
        contract Concrete is AbstractMiddle {
            function $6run() external override {}
        }
        abstract contract Unimplemented {
            function $7missing() external virtual;
        }

        interface Root {
            function $8run() external;
        }
        contract RootBase is Root {
            function $9run() public virtual {}
        }
        contract RootMiddle is RootBase {}
        contract RootLeaf is RootMiddle {
            function $10run() public override {}
        }
        interface Left {
            function $11ping() external;
        }
        interface Right {
            function $12ping() external;
        }
        contract Both is Left, Right {
            function $13ping() external override(Left, Right) {}
        }

        contract DirBase {
            function $14run() public virtual {}
        }
        contract DirMiddle is DirBase {
            function $15run() public virtual override {}
        }
        contract DirLeaf is DirMiddle {
            function $16run() public override {}
        }
        contract Unoverridden {
            function $17standalone() public virtual {}
        }

        interface Picker {
            function $18pick(uint256 value) external;
            function $19pick(string calldata value) external;
        }
        contract PickerImpl is Picker {
            function $20pick(uint256 value) public override {}
            function $21pick(string calldata value) public override {}
            function call() public {
                $22pick(uint256(1));
            }
        }

        contract Standalone {
            function $23target() public {}
            function call() public {
                $24target();
            }
        }

        abstract contract GetterBase {
            function $25value() external view virtual returns (uint256);
        }
        contract GetterChild is GetterBase {
            uint256 public override $26value;
            function read() external view returns (uint256) {
                return this.$27value();
            }
        }

        contract ModBase {
            modifier $28guard() virtual { _; }
        }
        contract ModChild is ModBase {
            modifier $29guard() override { _; }
            function run() public $30guard {}
        }

        contract $31Container {
            struct $32Data { uint256 value; }
            enum $33Choice { None, Some }
            event $34Changed(uint256 value);
            error $35Failure(uint256 value);
        }

        contract UsesAlias {
            $39Alias value;
            $40NS.Aliased other;
        }
        "#,
        "/Main.sol",
    );

    fixture.check_queries(
        &[Query::Implementation],
        1..=40,
        str![[r#"
$1 /Main.sol:6:13 function run() external override {}
/Main.sol:9:13 function run() external override {}
$2 <none>
$3 <none>
$4 /Main.sol:18:13 function run() external override {}
$5 /Main.sol:18:13 function run() external override {}
$6 <none>
$7 <none>
$8 /Main.sol:27:13 function run() public virtual {}
/Main.sol:31:13 function run() public override {}
$9 /Main.sol:31:13 function run() public override {}
$10 <none>
$11 /Main.sol:40:13 function ping() external override(Left, Right) {}
$12 /Main.sol:40:13 function ping() external override(Left, Right) {}
$13 <none>
$14 /Main.sol:46:13 function run() public virtual override {}
/Main.sol:49:13 function run() public override {}
$15 /Main.sol:49:13 function run() public override {}
$16 <none>
$17 <none>
$18 /Main.sol:59:13 function pick(uint256 value) public override {}
$19 /Main.sol:60:13 function pick(string calldata value) public override {}
$20 <none>
$21 <none>
$22 /Main.sol:59:13 function pick(uint256 value) public override {}
$23 /Main.sol:66:13 function target() public {}
$24 /Main.sol:66:13 function target() public {}
$25 /Main.sol:75:28 uint256 public override value;
$26 <none>
$27 /Main.sol:75:28 uint256 public override value;
$28 /Main.sol:84:13 modifier guard() override { _; }
$29 <none>
$30 /Main.sol:84:13 modifier guard() override { _; }
$31 /Main.sol:87:9 contract Container {
$32 /Main.sol:88:11 struct Data { uint256 value; }
$33 /Main.sol:89:9 enum Choice { None, Some }
$34 /Main.sol:90:10 event Changed(uint256 value);
$35 /Main.sol:91:10 error Failure(uint256 value);
$36 /Base.sol:0:9 contract Aliased {}
$37 /Base.sol:0:9 contract Aliased {}
$38 <none>
$39 /Base.sol:0:9 contract Aliased {}
$40 <none>

"#]],
    );
}

#[test]
fn merges_implementations_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Base.sol
        interface I {
            function $1run() external;
        }
        abstract contract GetterBase {
            function $4value() external view virtual returns (uint256);
        }

        //- /FirstBase.sol
        contract FirstBase {}

        //- /SecondBase.sol
        contract $7SecondBase {}

        //- /First.sol
        import "./Base.sol";
        import {FirstBase as FirstAlias} from "./FirstBase.sol";
        contract First is I, GetterBase {
            function $2run() external override {}
            uint256 public override $5value;
            FirstAlias firstAlias;
        }

        //- /Second.sol
        import "./Base.sol";
        import {SecondBase as $8Alias} from "./SecondBase.sol";
        contract Second is I, GetterBase {
            function $3run() external override {}
            uint256 public override $6value;
            $9Alias secondAlias;
        }
        "#,
        &["/First.sol", "/Second.sol"],
    );

    fixture.check_queries(
        &[Query::Implementation],
        1..=9,
        str![[r#"
$1 /First.sol:3:13 function run() external override {}
/Second.sol:3:13 function run() external override {}
$2 <none>
$3 <none>
$4 /First.sol:4:28 uint256 public override value;
/Second.sol:4:28 uint256 public override value;
$5 <none>
$6 <none>
$7 /SecondBase.sol:0:9 contract SecondBase {}
$8 /SecondBase.sol:0:9 contract SecondBase {}
$9 /SecondBase.sol:0:9 contract SecondBase {}

"#]],
    );
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
        fixture.check_queries(&[Query::Implementation], [1], "$1 <none>\n");
    }
}

#[test]
fn isolates_conflicting_dependency_implementations_across_analysis_batches() {
    let source = r#"
        //- /Shared.sol open
        abstract contract Base {
            function run(uint256) public virtual {}
        }

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
        fixture.check_queries(
            &[Query::Implementation],
            [1],
            str![[r#"
$1 /first/Main.sol:2:13 function run(bytes32) public override {}

"#]],
        );
    }
}
