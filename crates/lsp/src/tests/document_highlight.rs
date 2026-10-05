use super::support::{Query, RequestFixture};
use snapbox::str;

#[test]
fn classifies_reads_writes_and_nested_lvalues() {
    let fixture = RequestFixture::new(
        r#"
        //- /Kinds.sol
        contract C {
            struct Box { uint256 $2field; }

            uint256 $1value;
            uint256 $4index;
            mapping(uint256 => Box) $3boxes;
            mapping(uint256 => uint256) $5items;

            function update() external returns (uint256) {
                uint256 read = value;
                value = read;
                value += 1;
                delete value;
                ++value;
                value--;
                boxes[index].field += 1;
                items[index] = value;
                return value;
            }
        }
        "#,
        "/Kinds.sol",
    );

    fixture.check_queries(
        &[Query::Highlights],
        1..=5,
        str![[r#"
$1 2:12-2:17 WRITE
7:23-7:28 READ
8:8-8:13 WRITE
9:8-9:13 WRITE
10:15-10:20 WRITE
11:10-11:15 WRITE
12:8-12:13 WRITE
14:23-14:28 READ
15:15-15:20 READ
$2 1:25-1:30 WRITE
13:21-13:26 WRITE
$3 4:28-4:33 WRITE
13:8-13:13 READ
$4 3:12-3:17 WRITE
13:14-13:19 READ
14:14-14:19 READ
$5 5:32-5:37 WRITE
14:8-14:13 READ

"#]],
    );
}

#[test]
fn scopes_semantic_matches_to_the_requested_document() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        contract Base {
            uint256 internal shared;

            function baseRead() public view returns (uint256) {
                return shared;
            }
        }

        //- /Use.sol
        import {Base} from "./Base.sol";
        contract Use is Base {
            uint256 local;

            function write(uint256 input) external {
                $1shared = input;
                local = local + local;
                local = local + local;
            }
            function read() external view returns (uint256) {
                return shared;
            }
            function shadow(uint256 $2shared) external pure returns (uint256) {
                return shared;
            }
        }
        "#,
        "/Use.sol",
    );

    fixture.check_queries(
        &[Query::Highlights],
        [1, 2],
        str![[r#"
$1 4:8-4:14 WRITE
9:15-9:21 READ
$2 11:28-11:34 WRITE
12:15-12:21 READ

"#]],
    );
}

#[test]
fn preserves_ambiguous_reference_targets() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Ambiguous.sol
        contract C {
            function pick(uint8 value) internal pure returns (uint8) {
                return value;
            }

            function pick(uint256 value) internal pure returns (uint256) {
                return value;
            }

            function call(uint8 value) public pure returns (uint256) {
                return $1pick(value);
            }
        }
        "#,
        "/Ambiguous.sol",
    );

    fixture.check_queries(
        &[Query::References(true), Query::Highlights],
        [1],
        str![[r#"
$1 references: /Ambiguous.sol:1:13 function pick(uint8 value) internal pure returns (uint8) {
/Ambiguous.sol:4:13 function pick(uint256 value) internal pure returns (uint256) {
/Ambiguous.sol:8:15 return pick(value);
$1 highlights: 1:13-1:17 WRITE
4:13-4:17 WRITE
8:15-8:19 READ

"#]],
    );
}

#[test]
fn preserves_references_across_analysis_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /First.sol
        contract First {
            uint256 $3value;
            function read() public view returns (uint256) {
                return $4value;
            }
        }

        //- /Second.sol
        contract Second {
            uint256 $1value;
            function $5write(address account) public returns (address) {
                $2value = 1;
                return account;
            }
        }
        "#,
        &["/First.sol", "/Second.sol"],
    );

    fixture.check_queries(
        &[Query::References(true), Query::Highlights, Query::Hover],
        1..=5,
        str![[r#"
$1 references: /Second.sol:1:12 uint256 value;
/Second.sol:3:8 value = 1;
$1 highlights: 1:12-1:17 WRITE
3:8-3:13 WRITE
$1 hover: 1:12-1:17 uint256 value
$2 references: /Second.sol:1:12 uint256 value;
/Second.sol:3:8 value = 1;
$2 highlights: 1:12-1:17 WRITE
3:8-3:13 WRITE
$2 hover: 3:8-3:13 uint256 value
$3 references: /First.sol:1:12 uint256 value;
/First.sol:3:15 return value;
$3 highlights: 1:12-1:17 WRITE
3:15-3:20 READ
$3 hover: 1:12-1:17 uint256 value
$4 references: /First.sol:1:12 uint256 value;
/First.sol:3:15 return value;
$4 highlights: 1:12-1:17 WRITE
3:15-3:20 READ
$4 hover: 3:15-3:20 uint256 value
$5 references: /Second.sol:2:13 function write(address account) public returns (address) {
$5 highlights: 2:13-2:18 WRITE
$5 hover: 2:13-2:18 function write(address account) public returns (address)

"#]],
    );
}
