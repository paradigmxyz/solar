use super::support::{Query, RequestFixture};
use snapbox::str;

#[test]
fn resolves_declared_and_referenced_types() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol
        import {Shared} from "./Types.sol";
        import "./Late.sol";

        interface $1InterfaceType {}
        library $2LibraryType {}
        struct $3StructType { uint256 value; }
        enum $4EnumType { A }
        type $5ValueType is uint256;
        error Failed(StructType detail);

        contract $6C {
            struct Second { uint256 value; }
            struct First { uint256 value; }
            struct NumberResult { uint256 value; }
            struct TextResult { string value; }

            Shared $7shared;
            First $8stored;
            First[][] $9nested;
            mapping(EnumType => First[]) $10values;
            mapping(uint256 => Second) public getterValues;

            function $11pair() public pure returns (First memory a, Second memory b, First memory c) {}
            function pick(uint256) public pure returns (NumberResult memory r) {}
            function pick(string memory) public pure returns (TextResult memory r) {}

            function use(First memory $12input) external {
                First memory $13local = $14input;
                $15stored = $16local;
                $17pair();
                $18pick(uint256(1));
                this.$19getterValues(1);
                revert Failed({ $20detail: StructType({ value: 1 }) });
            }
        }

        //- /Types.sol
        struct Shared { uint256 value; }

        //- /Late.sol
        contract UsesLater {
            Later $21value;
        }
        struct Later { uint256 value; }
        "#,
        "/Main.sol",
    );

    fixture.check_queries(
        &[Query::TypeDefinition],
        1..=21,
        str![[r#"
$1 /Main.sol:2:10 interface InterfaceType {}
$2 /Main.sol:3:8 library LibraryType {}
$3 /Main.sol:4:7 struct StructType { uint256 value; }
$4 /Main.sol:5:5 enum EnumType { A }
$5 /Main.sol:6:5 type ValueType is uint256;
$6 /Main.sol:8:9 contract C {
$7 /Types.sol:0:7 struct Shared { uint256 value; }
$8 /Main.sol:10:11 struct First { uint256 value; }
$9 /Main.sol:10:11 struct First { uint256 value; }
$10 /Main.sol:10:11 struct First { uint256 value; }
$11 /Main.sol:10:11 struct First { uint256 value; }
/Main.sol:9:11 struct Second { uint256 value; }
$12 /Main.sol:10:11 struct First { uint256 value; }
$13 /Main.sol:10:11 struct First { uint256 value; }
$14 /Main.sol:10:11 struct First { uint256 value; }
$15 /Main.sol:10:11 struct First { uint256 value; }
$16 /Main.sol:10:11 struct First { uint256 value; }
$17 /Main.sol:10:11 struct First { uint256 value; }
/Main.sol:9:11 struct Second { uint256 value; }
$18 /Main.sol:11:11 struct NumberResult { uint256 value; }
$19 /Main.sol:9:11 struct Second { uint256 value; }
$20 /Main.sol:4:7 struct StructType { uint256 value; }
$21 /Late.sol:3:7 struct Later { uint256 value; }

"#]],
    );
}

#[test]
fn preserves_type_definitions_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /first/Types.sol
        struct FirstType { uint256 value; }

        //- /first/Main.sol
        import "./Types.sol";
        contract First { FirstType $1value; }

        //- /second/Types.sol
        struct SecondType { uint256 value; }

        //- /second/Main.sol
        import "./Types.sol";
        contract Second { SecondType $2value; }
        "#,
        &["/first/Main.sol", "/second/Main.sol"],
    );

    fixture.check_queries(
        &[Query::TypeDefinition],
        [1, 2],
        str![[r#"
$1 /first/Types.sol:0:7 struct FirstType { uint256 value; }
$2 /second/Types.sol:0:7 struct SecondType { uint256 value; }

"#]],
    );
}

#[test]
fn primitive_function_and_unresolved_types_have_no_target() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /NoTarget.sol
        struct Detail { uint256 code; }
        error $5Error(Detail detail);
        contract C {
            event $4Event(Detail detail);
            uint256 $1count;

            function use(function(uint256) external returns (uint256) $2callback) public {
                Missing $3missing;
            }

            modifier $6Modifier(Detail detail) {
                _;
            }
        }
        "#,
        "/NoTarget.sol",
    );

    fixture.check_queries(
        &[Query::TypeDefinition],
        1..=6,
        str![[r#"
$1 <none>
$2 <none>
$3 <none>
$4 <none>
$5 <none>
$6 <none>

"#]],
    );
}
