use super::support::{Query, RequestFixture};
use snapbox::str;

#[test]
fn resolves_namespace_chains_across_comments() {
    let fixture = RequestFixture::new(
        r#"
        //- /Lib.sol
        struct Pos { uint256 x; }

        //- /Middle.sol
        import "./Lib.sol" as A;

        //- /Main.sol
        import "./Middle.sol" as B;
        contract Use {
            $1B /* A */ . $2A /* Pos */ . $3Pos value;
        }
        "#,
        "/Main.sol",
    );

    fixture.check_queries(
        &[Query::Definition, Query::Hover, Query::References(true), Query::Highlights],
        1..=3,
        str![[r#"
$1 definition: /Main.sol:0:25 import "./Middle.sol" as B;
$1 hover: 2:4-2:5 import "./Middle.sol" as B;
$1 references: /Main.sol:0:25 import "./Middle.sol" as B;
/Main.sol:2:4 B /* A */ . A /* Pos */ . Pos value;
$1 highlights: 0:25-0:26 WRITE
2:4-2:5 READ
$2 definition: /Middle.sol:0:22 import "./Lib.sol" as A;
$2 hover: 2:16-2:17 import "./Lib.sol" as A;
$2 references: /Main.sol:2:16 B /* A */ . A /* Pos */ . Pos value;
/Middle.sol:0:22 import "./Lib.sol" as A;
$2 highlights: 2:16-2:17 READ
$3 definition: /Lib.sol:0:7 struct Pos { uint256 x; }
$3 hover: 2:30-2:33 struct Pos
$3 references: /Lib.sol:0:7 struct Pos { uint256 x; }
/Main.sol:2:30 B /* A */ . A /* Pos */ . Pos value;
$3 highlights: 2:30-2:33 READ

"#]],
    );
    fixture.check_prepare_rename("$3", "2:30-2:33\n");
    fixture.check_renames(
        &[("$2", "Renamed"), ("$3", "Renamed")],
        str![[r#"
$2:
/Main.sol:2:16-2:17 -> Renamed
/Middle.sol:0:22-0:23 -> Renamed
$3:
/Lib.sol:0:7-0:10 -> Renamed
/Main.sol:2:30-2:33 -> Renamed

"#]],
    );
}

#[test]
fn keeps_two_aliases_for_one_namespace_independent() {
    let fixture = RequestFixture::new(
        r#"
        //- /Lib.sol
        struct Pos { uint256 x; }

        //- /Main.sol
        import "./Lib.sol" as $1A;
        import "./Lib.sol" as $2B;
        contract Use {
            $3A.$5Pos first;
            $4B.Pos second;
        }
        "#,
        "/Main.sol",
    );

    fixture.check_queries(
        &[Query::References(true)],
        1..=5,
        str![[r#"
$1 /Main.sol:0:22 import "./Lib.sol" as A;
/Main.sol:3:4 A.Pos first;
$2 /Main.sol:1:22 import "./Lib.sol" as B;
/Main.sol:4:4 B.Pos second;
$3 /Main.sol:0:22 import "./Lib.sol" as A;
/Main.sol:3:4 A.Pos first;
$4 /Main.sol:1:22 import "./Lib.sol" as B;
/Main.sol:4:4 B.Pos second;
$5 /Lib.sol:0:7 struct Pos { uint256 x; }
/Main.sol:3:6 A.Pos first;
/Main.sol:4:6 B.Pos second;

"#]],
    );
    fixture.check_queries(
        &[Query::Highlights],
        [3],
        str![[r#"
$3 0:22-0:23 WRITE
3:4-3:5 READ

"#]],
    );
    fixture.check_renames(
        &[("$3", "Renamed"), ("$4", "Renamed")],
        str![[r#"
$3:
/Main.sol:0:22-0:23 -> Renamed
/Main.sol:3:4-3:5 -> Renamed
$4:
/Main.sol:1:22-1:23 -> Renamed
/Main.sol:4:4-4:5 -> Renamed

"#]],
    );
}

#[test]
fn preserves_overload_selection_for_events_and_qualified_reverts() {
    let fixture = RequestFixture::new(
        r#"
        //- /Events.sol
        interface Events {
            event $1Hit(uint256 value);
            event $2Hit(address value);
            error Failed(uint256 code);
        }
        contract Use {
            function run(address who) external {
                emit $3Events.$4Hit(uint256(1));
                emit Events.$5Hit(who);
                revert $6Events.$7Failed(7);
            }
        }
        "#,
        "/Events.sol",
    );

    fixture.check_queries(
        &[Query::Definition, Query::Hover],
        [4, 5, 7],
        str![[r#"
$4 definition: /Events.sol:1:10 event Hit(uint256 value);
$4 hover: 7:20-7:23 event Hit(uint256 value)
$5 definition: /Events.sol:2:10 event Hit(address value);
$5 hover: 8:20-8:23 event Hit(address value)
$7 definition: /Events.sol:3:10 error Failed(uint256 code);
$7 hover: 9:22-9:28 error Failed(uint256 code)

"#]],
    );
    fixture.check_queries(
        &[Query::References(true)],
        [1, 2, 3, 6],
        str![[r#"
$1 /Events.sol:1:10 event Hit(uint256 value);
/Events.sol:7:20 emit Events.Hit(uint256(1));
$2 /Events.sol:2:10 event Hit(address value);
/Events.sol:8:20 emit Events.Hit(who);
$3 /Events.sol:0:10 interface Events {
/Events.sol:7:13 emit Events.Hit(uint256(1));
/Events.sol:8:13 emit Events.Hit(who);
/Events.sol:9:15 revert Events.Failed(7);
$6 /Events.sol:0:10 interface Events {
/Events.sol:7:13 emit Events.Hit(uint256(1));
/Events.sol:8:13 emit Events.Hit(who);
/Events.sol:9:15 revert Events.Failed(7);

"#]],
    );
    fixture.check_renames(
        &[("$4", "Renamed"), ("$7", "Renamed")],
        str![[r#"
$4:
/Events.sol:1:10-1:13 -> Renamed
/Events.sol:7:20-7:23 -> Renamed
$7:
/Events.sol:3:10-3:16 -> Renamed
/Events.sol:9:22-9:28 -> Renamed

"#]],
    );
}

#[test]
fn distinguishes_inherited_type_qualifier_from_declaring_contract() {
    let fixture = RequestFixture::new(
        r#"
        //- /Inherited.sol
        contract Base { struct $1S { uint256 field; } }
        contract $2Child is Base {}
        contract Use {
            $3Child.$4S inherited;
            Base.S direct;
        }
        "#,
        "/Inherited.sol",
    );

    fixture.check_queries(
        &[Query::Definition, Query::Hover],
        [3, 4],
        str![[r#"
$3 definition: /Inherited.sol:1:9 contract Child is Base {}
$3 hover: 3:4-3:9 contract Child is Base
$4 definition: /Inherited.sol:0:23 contract Base { struct S { uint256 field; } }
$4 hover: 3:10-3:11 struct S

"#]],
    );
    fixture.check_queries(
        &[Query::References(true)],
        [1, 2],
        str![[r#"
$1 /Inherited.sol:0:23 contract Base { struct S { uint256 field; } }
/Inherited.sol:3:10 Child.S inherited;
/Inherited.sol:4:9 Base.S direct;
$2 /Inherited.sol:1:9 contract Child is Base {}
/Inherited.sol:3:4 Child.S inherited;

"#]],
    );
    fixture.check_rename(
        "$4",
        "Renamed",
        str![[r#"
/Inherited.sol:0:23-0:24 -> Renamed
/Inherited.sol:3:10-3:11 -> Renamed
/Inherited.sol:4:9-4:10 -> Renamed

"#]],
    );
}

#[test]
fn deduplicates_qualified_paths_across_analysis_batches() {
    let source = r#"
        //- /Lib.sol
        struct Pos { uint256 x; }

        //- /Shared.sol
        import "./Lib.sol" as $1NS;
        contract Shared { $2NS.$3Pos value; }

        //- /First.sol
        import "./Shared.sol";
        contract First is Shared {}

        //- /Second.sol
        import "./Shared.sol";
        contract Second is Shared {}
    "#;
    for paths in [["/First.sol", "/Second.sol"], ["/Second.sol", "/First.sol"]] {
        let fixture = RequestFixture::new_in_batches(source, &paths);
        fixture.check_queries(
            &[Query::Definition, Query::Hover, Query::References(true), Query::Highlights],
            [2],
            str![[r#"
$2 definition: /Shared.sol:0:22 import "./Lib.sol" as NS;
$2 hover: 1:18-1:20 import "./Lib.sol" as NS;
$2 references: /Shared.sol:0:22 import "./Lib.sol" as NS;
/Shared.sol:1:18 contract Shared { NS.Pos value; }
$2 highlights: 0:22-0:24 WRITE
1:18-1:20 READ

"#]],
        );
        fixture.check_queries(
            &[Query::References(true)],
            [1, 3],
            str![[r#"
$1 /Shared.sol:0:22 import "./Lib.sol" as NS;
/Shared.sol:1:18 contract Shared { NS.Pos value; }
$3 /Lib.sol:0:7 struct Pos { uint256 x; }
/Shared.sol:1:21 contract Shared { NS.Pos value; }

"#]],
        );
        fixture.check_rename(
            "$2",
            "Renamed",
            str![[r#"
/Shared.sol:0:22-0:24 -> Renamed
/Shared.sol:1:18-1:20 -> Renamed

"#]],
        );
    }
}
