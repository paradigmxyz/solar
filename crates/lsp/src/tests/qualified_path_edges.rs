use super::*;
use snapbox::str;
use support::RequestFixture;

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

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Main.sol:0:25 import "./Middle.sol" as B;

"#]],
    );
    fixture.check_goto_definition(
        "$2",
        str![[r#"
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_goto_definition(
        "$3",
        str![[r#"
/Lib.sol:0:7 struct Pos { uint256 x; }

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
2:16-2:17
```solidity
import "./Lib.sol" as A;
```

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Main.sol:2:16 B /* A */ . A /* Pos */ . Pos value;
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_document_highlights(
        "$2",
        str![[r#"
2:16-2:17 READ

"#]],
    );
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/Main.sol:2:16-2:17 -> Renamed
/Middle.sol:0:22-0:23 -> Renamed

"#]],
    );
    fixture.check_prepare_rename("$3", "2:30-2:33\n");
    fixture.check_rename(
        "$3",
        "Renamed",
        str![[r#"
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

    let references_a = str![[r#"
/Main.sol:0:22 import "./Lib.sol" as A;
/Main.sol:3:4 A.Pos first;

"#]];
    fixture.check_references("$1", true, references_a.clone());
    fixture.check_references("$3", true, references_a);
    let references_b = str![[r#"
/Main.sol:1:22 import "./Lib.sol" as B;
/Main.sol:4:4 B.Pos second;

"#]];
    fixture.check_references("$2", true, references_b.clone());
    fixture.check_references("$4", true, references_b);
    fixture.check_document_highlights(
        "$3",
        str![[r#"
0:22-0:23 WRITE
3:4-3:5 READ

"#]],
    );
    fixture.check_rename(
        "$3",
        "Renamed",
        str![[r#"
/Main.sol:0:22-0:23 -> Renamed
/Main.sol:3:4-3:5 -> Renamed

"#]],
    );
    fixture.check_rename(
        "$4",
        "Renamed",
        str![[r#"
/Main.sol:1:22-1:23 -> Renamed
/Main.sol:4:4-4:5 -> Renamed

"#]],
    );
    fixture.check_references(
        "$5",
        true,
        str![[r#"
/Lib.sol:0:7 struct Pos { uint256 x; }
/Main.sol:3:6 A.Pos first;
/Main.sol:4:6 B.Pos second;

"#]],
    );
}

#[test]
fn resolves_nested_namespace_members_in_function_calls() {
    let fixture = RequestFixture::new(
        r#"
        //- /Lib.sol
        function make() pure returns (uint256) { return 1; }

        //- /Middle.sol
        import "./Lib.sol" as A;

        //- /Main.sol
        import "./Middle.sol" as B;
        contract Use {
            function get() external pure returns (uint256) {
                return $1B.$2A.$3make();
            }
        }
        "#,
        "/Main.sol",
    );

    fixture.check_goto_definition(
        "$2",
        str![[r#"
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
3:17-3:18
```solidity
import "./Lib.sol" as A;
```

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Main.sol:3:17 return B.A.make();
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_rename(
        "$2",
        "Renamed",
        str![[r#"
/Main.sol:3:17-3:18 -> Renamed
/Middle.sol:0:22-0:23 -> Renamed

"#]],
    );
    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Main.sol:0:25 import "./Middle.sol" as B;

"#]],
    );
    fixture.check_goto_definition(
        "$3",
        str![[r#"
/Lib.sol:0:9 function make() pure returns (uint256) { return 1; }

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

    fixture.check_goto_definition(
        "$4",
        str![[r#"
/Events.sol:1:10 event Hit(uint256 value);

"#]],
    );
    fixture.check_hover(
        "$4",
        str![[r#"
7:20-7:23
```solidity
event Hit(uint256 value)
```

"#]],
    );
    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Events.sol:1:10 event Hit(uint256 value);
/Events.sol:7:20 emit Events.Hit(uint256(1));

"#]],
    );
    fixture.check_rename(
        "$4",
        "Renamed",
        str![[r#"
/Events.sol:1:10-1:13 -> Renamed
/Events.sol:7:20-7:23 -> Renamed

"#]],
    );
    fixture.check_goto_definition(
        "$5",
        str![[r#"
/Events.sol:2:10 event Hit(address value);

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Events.sol:2:10 event Hit(address value);
/Events.sol:8:20 emit Events.Hit(who);

"#]],
    );
    fixture.check_goto_definition(
        "$7",
        str![[r#"
/Events.sol:3:10 error Failed(uint256 code);

"#]],
    );
    fixture.check_hover(
        "$7",
        str![[r#"
9:22-9:28
```solidity
error Failed(uint256 code)
```

"#]],
    );
    fixture.check_rename(
        "$7",
        "Renamed",
        str![[r#"
/Events.sol:3:10-3:16 -> Renamed
/Events.sol:9:22-9:28 -> Renamed

"#]],
    );
    let qualifiers = str![[r#"
/Events.sol:0:10 interface Events {
/Events.sol:7:13 emit Events.Hit(uint256(1));
/Events.sol:8:13 emit Events.Hit(who);
/Events.sol:9:15 revert Events.Failed(7);

"#]];
    fixture.check_references("$3", true, qualifiers.clone());
    fixture.check_references("$6", true, qualifiers);
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

    fixture.check_goto_definition(
        "$3",
        str![[r#"
/Inherited.sol:1:9 contract Child is Base {}

"#]],
    );
    fixture.check_hover(
        "$3",
        str![[r#"
3:4-3:9
```solidity
contract Child is Base
```

"#]],
    );
    fixture.check_goto_definition(
        "$4",
        str![[r#"
/Inherited.sol:0:23 contract Base { struct S { uint256 field; } }

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Inherited.sol:1:9 contract Child is Base {}
/Inherited.sol:3:4 Child.S inherited;

"#]],
    );
    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Inherited.sol:0:23 contract Base { struct S { uint256 field; } }
/Inherited.sol:3:10 Child.S inherited;
/Inherited.sol:4:9 Base.S direct;

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
        fixture.check_goto_definition(
            "$2",
            str![[r#"
/Shared.sol:0:22 import "./Lib.sol" as NS;

"#]],
        );
        fixture.check_hover(
            "$2",
            str![[r#"
1:18-1:20
```solidity
import "./Lib.sol" as NS;
```

"#]],
        );
        let references = str![[r#"
/Shared.sol:0:22 import "./Lib.sol" as NS;
/Shared.sol:1:18 contract Shared { NS.Pos value; }

"#]];
        fixture.check_references("$1", true, references.clone());
        fixture.check_references("$2", true, references);
        fixture.check_document_highlights(
            "$2",
            str![[r#"
0:22-0:24 WRITE
1:18-1:20 READ

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
        fixture.check_references(
            "$3",
            true,
            str![[r#"
/Lib.sol:0:7 struct Pos { uint256 x; }
/Shared.sol:1:21 contract Shared { NS.Pos value; }

"#]],
        );
    }
}

#[test]
fn resolves_namespace_aliases_through_plain_imports() {
    let source = r#"
        //- /Lib.sol
        struct Pos { uint256 x; }

        //- /Middle.sol
        import "./Lib.sol" as $1A;

        //- /Bridge.sol
        import "./Middle.sol";

        //- /Main.sol
        import "./Bridge.sol";
        contract Use { $2A.Pos value; }
    "#;
    for paths in [["/Main.sol", "/Middle.sol"], ["/Middle.sol", "/Main.sol"]] {
        let fixture = RequestFixture::new_in_batches(source, &paths);
        fixture.check_goto_definition(
            "$2",
            str![[r#"
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]],
        );
        fixture.check_hover(
            "$2",
            str![[r#"
1:15-1:16
```solidity
import "./Lib.sol" as A;
```

"#]],
        );
        let references = str![[r#"
/Main.sol:1:15 contract Use { A.Pos value; }
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]];
        fixture.check_references("$1", true, references.clone());
        fixture.check_references("$2", true, references);
        let rename = str![[r#"
/Main.sol:1:15-1:16 -> Renamed
/Middle.sol:0:22-0:23 -> Renamed

"#]];
        fixture.check_rename("$1", "Renamed", rename.clone());
        fixture.check_rename("$2", "Renamed", rename);
    }
}

#[test]
fn distinguishes_renamed_namespace_reexports_and_original_names() {
    let fixture = RequestFixture::new(
        r#"
        //- /Lib.sol
        struct Pos { uint256 x; }

        //- /Middle.sol
        import "./Lib.sol" as $1A;

        //- /Main.sol
        import {$2A as $3B} from "./Middle.sol";
        contract Main { $4B.Pos value; }

        //- /Leaf.sol
        import {$5B as $6C} from "./Main.sol";
        contract Leaf { $7C.Pos value; }
        "#,
        "/Leaf.sol",
    );

    fixture.check_goto_definition(
        "$4",
        str![[r#"
/Main.sol:0:13 import {A as B} from "./Middle.sol";

"#]],
    );
    fixture.check_hover(
        "$4",
        str![[r#"
1:16-1:17
```solidity
import {A as B} from "./Middle.sol";
```

"#]],
    );
    let references_b = str![[r#"
/Leaf.sol:0:8 import {B as C} from "./Main.sol";
/Main.sol:0:13 import {A as B} from "./Middle.sol";
/Main.sol:1:16 contract Main { B.Pos value; }

"#]];
    fixture.check_references("$3", true, references_b.clone());
    fixture.check_references("$4", true, references_b.clone());
    fixture.check_references("$5", true, references_b);
    let rename_b = str![[r#"
/Leaf.sol:0:8-0:9 -> Renamed
/Main.sol:0:13-0:14 -> Renamed
/Main.sol:1:16-1:17 -> Renamed

"#]];
    fixture.check_rename("$3", "Renamed", rename_b.clone());
    fixture.check_rename("$4", "Renamed", rename_b.clone());
    fixture.check_rename("$5", "Renamed", rename_b);
    fixture.check_goto_definition(
        "$2",
        str![[r#"
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_hover(
        "$2",
        str![[r#"
0:8-0:9
```solidity
import "./Lib.sol" as A;
```

"#]],
    );
    let references_a = str![[r#"
/Main.sol:0:8 import {A as B} from "./Middle.sol";
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]];
    fixture.check_references("$1", true, references_a.clone());
    fixture.check_references("$2", true, references_a);
    let rename_a = str![[r#"
/Main.sol:0:8-0:9 -> Renamed
/Middle.sol:0:22-0:23 -> Renamed

"#]];
    fixture.check_rename("$1", "Renamed", rename_a.clone());
    fixture.check_rename("$2", "Renamed", rename_a);
    fixture.check_goto_definition(
        "$7",
        str![[r#"
/Leaf.sol:0:13 import {B as C} from "./Main.sol";

"#]],
    );
    fixture.check_references(
        "$6",
        true,
        str![[r#"
/Leaf.sol:0:13 import {B as C} from "./Main.sol";
/Leaf.sol:1:16 contract Leaf { C.Pos value; }

"#]],
    );
    fixture.check_rename(
        "$7",
        "Renamed",
        str![[r#"
/Leaf.sol:0:13-0:14 -> Renamed
/Leaf.sol:1:16-1:17 -> Renamed

"#]],
    );
}

#[test]
fn preserves_prior_namespace_aliases_when_reexports_follow() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Lib.sol
        struct Pos { uint256 x; }

        //- /Middle.sol
        import "./Lib.sol" as $1A;

        //- /Plain.sol
        import "./Lib.sol" as $2A;
        import "./Middle.sol";
        contract Plain { $3A.Pos value; }

        //- /Named.sol
        import "./Lib.sol" as $4A;
        import {$5A} from "./Middle.sol";
        contract Named { $6A.Pos value; }
        "#,
        &["/Plain.sol", "/Named.sol"],
    );

    fixture.check_goto_definition(
        "$3",
        str![[r#"
/Plain.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_goto_definition(
        "$6",
        str![[r#"
/Named.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_goto_definition(
        "$5",
        str![[r#"
/Middle.sol:0:22 import "./Lib.sol" as A;

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Plain.sol:0:22 import "./Lib.sol" as A;
/Plain.sol:2:17 contract Plain { A.Pos value; }

"#]],
    );
    fixture.check_rename(
        "$3",
        "Renamed",
        str![[r#"
/Plain.sol:0:22-0:23 -> Renamed
/Plain.sol:2:17-2:18 -> Renamed

"#]],
    );
    fixture.check_references(
        "$4",
        true,
        str![[r#"
/Named.sol:0:22 import "./Lib.sol" as A;
/Named.sol:2:17 contract Named { A.Pos value; }

"#]],
    );
    fixture.check_rename(
        "$6",
        "Renamed",
        str![[r#"
/Named.sol:0:22-0:23 -> Renamed
/Named.sol:2:17-2:18 -> Renamed

"#]],
    );
    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Middle.sol:0:22 import "./Lib.sol" as A;
/Named.sol:1:8 import {A} from "./Middle.sol";

"#]],
    );
    fixture.check_rename(
        "$1",
        "Renamed",
        str![[r#"
/Middle.sol:0:22-0:23 -> Renamed
/Named.sol:1:8-1:9 -> Renamed

"#]],
    );
}
