use super::support::RequestFixture;
use snapbox::str;

#[test]
fn finds_references_from_shared_dependency_across_batches() {
    let source = r#"
        //- /Shared.sol
        contract Base {}
        contract Shared {
            $1Base value;
        }

        //- /first/Main.sol
        import "../Shared.sol";
        contract First {
            Base value;
        }

        //- /second/Main.sol
        import "../Shared.sol";
        contract Second {
            Base value;
        }
        "#;

    for paths in [["/first/Main.sol", "/second/Main.sol"], ["/second/Main.sol", "/first/Main.sol"]]
    {
        let fixture = RequestFixture::new_in_batches(source, &paths);

        fixture.check_references(
            "$1",
            false,
            str![[r#"
/Shared.sol:2:4 Base value;
/first/Main.sol:2:4 Base value;
/second/Main.sol:2:4 Base value;

"#]],
        );
    }
}
