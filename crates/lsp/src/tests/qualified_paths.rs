use super::support::{Query, RequestFixture};
use snapbox::str;

#[test]
fn resolves_each_qualified_type_and_event_segment() {
    let fixture = RequestFixture::new(
        r#"
        //- /Lib.sol
        struct Pos { uint256 x; }

        //- /Main.sol
        import "./Lib.sol" as NS;
        interface IA { event Hit(uint256 v); }
        contract C {
            $1NS.$2Pos internal p;
            function emitHit() external {
                emit $3IA.$4Hit(1);
            }
        }
        "#,
        "/Main.sol",
    );

    fixture.check_queries(
        &[Query::Definition, Query::Hover, Query::References(true), Query::Highlights],
        1..=4,
        str![[r#"
$1 definition: /Main.sol:0:22 import "./Lib.sol" as NS;
$1 hover: 3:4-3:6
```solidity
import "./Lib.sol" as NS;
```
$1 references: /Main.sol:0:22 import "./Lib.sol" as NS;
/Main.sol:3:4 NS.Pos internal p;
$1 highlights: 0:22-0:24 WRITE
3:4-3:6 READ
$2 definition: /Lib.sol:0:7 struct Pos { uint256 x; }
$2 hover: 3:7-3:10
```solidity
struct Pos
```
$2 references: /Lib.sol:0:7 struct Pos { uint256 x; }
/Main.sol:3:7 NS.Pos internal p;
$2 highlights: 3:7-3:10 READ
$3 definition: /Main.sol:1:10 interface IA { event Hit(uint256 v); }
$3 hover: 5:13-5:15
```solidity
interface IA
```
$3 references: /Main.sol:1:10 interface IA { event Hit(uint256 v); }
/Main.sol:5:13 emit IA.Hit(1);
$3 highlights: 1:10-1:12 WRITE
5:13-5:15 READ
$4 definition: /Main.sol:1:21 interface IA { event Hit(uint256 v); }
$4 hover: 5:16-5:19
```solidity
event Hit(uint256 v)
```
$4 references: /Main.sol:1:21 interface IA { event Hit(uint256 v); }
/Main.sol:5:16 emit IA.Hit(1);
$4 highlights: 1:21-1:24 WRITE
5:16-5:19 READ

"#]],
    );
    fixture.check_prepare_rename("$1", "3:4-3:6\n");
    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Main.sol:0:22-0:24 -> Renamed
/Main.sol:3:4-3:6 -> Renamed

"#]],
    );

    fixture.check_prepare_rename("$2", "3:7-3:10\n");
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/Lib.sol:0:7-0:10 -> Renamed
/Main.sol:3:7-3:10 -> Renamed

"#]],
    );

    fixture.check_prepare_rename("$3", "5:13-5:15\n");
    fixture.check_rename(
        "$3",
        "Renamed",
        str![[r#"
/Main.sol:1:10-1:12 -> Renamed
/Main.sol:5:13-5:15 -> Renamed

"#]],
    );

    fixture.check_prepare_rename("$4", "5:16-5:19\n");
    fixture.check_rename(
        "$4",
        "Renamed",
        str![[r#"
/Main.sol:1:21-1:24 -> Renamed
/Main.sol:5:16-5:19 -> Renamed

"#]],
    );
}
