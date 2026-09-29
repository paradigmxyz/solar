use super::*;
use snapbox::str;

#[test]
fn resolves_each_qualified_type_and_event_segment() {
    let fixture = support::RequestFixture::new(
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

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Main.sol:0:22 import "./Lib.sol" as NS;

"#]],
    );
    fixture.check_hover(
        "$1",
        str![[r#"
3:4-3:6
```solidity
import "./Lib.sol" as NS;
```

"#]],
    );
    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Main.sol:0:22 import "./Lib.sol" as NS;
/Main.sol:3:4 NS.Pos internal p;

"#]],
    );
    fixture.check_document_highlights(
        "$1",
        str![[r#"
0:22-0:24 WRITE
3:4-3:6 READ

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

    fixture.check_goto_definition(
        "$2",
        str![[r#"
/Lib.sol:0:7 struct Pos { uint256 x; }

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
3:7-3:10
```solidity
struct Pos
```

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Lib.sol:0:7 struct Pos { uint256 x; }
/Main.sol:3:7 NS.Pos internal p;

"#]],
    );
    fixture.check_document_highlights(
        "$2",
        str![[r#"
3:7-3:10 READ

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

    fixture.check_goto_definition(
        "$3",
        str![[r#"
/Main.sol:1:10 interface IA { event Hit(uint256 v); }

"#]],
    );
    fixture.check_hover(
        "$3",
        str![[r#"
5:13-5:15
```solidity
interface IA
```

"#]],
    );
    fixture.check_references(
        "$3",
        true,
        str![[r#"
/Main.sol:1:10 interface IA { event Hit(uint256 v); }
/Main.sol:5:13 emit IA.Hit(1);

"#]],
    );
    fixture.check_document_highlights(
        "$3",
        str![[r#"
1:10-1:12 WRITE
5:13-5:15 READ

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

    fixture.check_goto_definition(
        "$4",
        str![[r#"
/Main.sol:1:21 interface IA { event Hit(uint256 v); }

"#]],
    );
    fixture.check_hover(
        "$4",
        str![[r#"
5:16-5:19
```solidity
event Hit(uint256 v)
```

"#]],
    );
    fixture.check_references(
        "$4",
        true,
        str![[r#"
/Main.sol:1:21 interface IA { event Hit(uint256 v); }
/Main.sol:5:16 emit IA.Hit(1);

"#]],
    );
    fixture.check_document_highlights(
        "$4",
        str![[r#"
1:21-1:24 WRITE
5:16-5:19 READ

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
