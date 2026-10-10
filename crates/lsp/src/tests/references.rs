use super::support::RequestFixture;
use snapbox::str;

#[test]
fn broken_for_initializer_does_not_rebind_loop_references() {
    for initializer in ["2", "* 2"] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                r#"
                //- /Loop.sol
                contract C {{
                    function caller(uint $1i) public pure returns (uint) {{
                        for (uint i = {initializer}; $2i < 3; i++) {{}}
                        return i;
                    }}
                }}
                "#
            ),
            "/Loop.sol",
        );
        fixture.check_references(
            "$1",
            true,
            str![[r#"
/Loop.sol:1:25 function caller(uint i) public pure returns (uint) {
/Loop.sol:3:15 return i;

"#]],
        );
        if initializer == "2" {
            fixture.check_goto_definition(
                "$2",
                str![[r#"
/Loop.sol:2:18 for (uint i = 2; i < 3; i++) {}

"#]],
            );
        } else {
            fixture.check_goto_definition("$2", "<none>\n");
        }
    }
}

#[test]
fn broken_control_statements_do_not_rebind_discarded_references() {
    for statement in [
        "for (uint i = 0; i < * 3; i++) { $2i++; }",
        "for (uint i = 0; i < 3; i += * 2) { $2i++; }",
        "for (uint i = * 2; i < 3; i++) $2i++;",
        "for (uint i = 0; i < * 3; i++) $2i++;",
        "for (uint i = 0; i < 3; i += * 2) $2i++;",
        "if (true) for (uint i = * 2; i < 3; i++) $2i++; else i++;",
        "for (uint i = * 2; i < 3; i++) if (true) $2i++; else i++;",
        "while (true) for (uint i = * 2; i < 3; i++) $2i++;",
        "do for (uint i = * 2; i < 3; i++) $2i++; while (false);",
        "for (uint i = * 2; i < 3; i++) do $2i++; while (false);",
        "try this.caller(* 2) returns (uint i) { $2i++; } catch { i++; }",
        "try this.caller(0) returns (uint i, * 2) { $2i++; } catch { i++; }",
        "try this.caller(0) returns (uint i) { $2i++; } catch Error(* 2) { i++; }",
    ] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                r#"
                //- /Control.sol
                contract C {{
                    function caller(uint $1i) public returns (uint) {{
                        {statement}
                        return i;
                    }}
                }}
                "#
            ),
            "/Control.sol",
        );
        fixture.check_references(
            "$1",
            true,
            str![[r#"
/Control.sol:1:25 function caller(uint i) public returns (uint) {
/Control.sol:3:15 return i;

"#]],
        );
        fixture.check_goto_definition("$2", "<none>\n");
    }
}

#[test]
fn mismatched_for_delimiters_preserve_following_function() {
    // An unmatched delimiter makes the rest of this function's scope uncertain. Keep the
    // following function's symbols without indexing any of the uncertain statements.
    for initializer in ["(* 2", "[* 2"] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            &format!(
                r#"
                //- /Delimiters.sol
                contract C {{
                    function broken(uint $1i) public pure returns (uint) {{
                        for (uint i = {initializer}; i < 3; i++) {{}}
                        return i;
                    }}
                    function next(uint $3j) public pure returns (uint) {{
                        return $4j;
                    }}
                }}
                "#
            ),
            "/Delimiters.sol",
        );
        fixture.check_references(
            "$1",
            true,
            str![[r#"
/Delimiters.sol:1:25 function broken(uint i) public pure returns (uint) {

"#]],
        );
        fixture.check_references(
            "$3",
            true,
            str![[r#"
/Delimiters.sol:5:23 function next(uint j) public pure returns (uint) {
/Delimiters.sol:6:15 return j;

"#]],
        );
        fixture.check_goto_definition(
            "$4",
            str![[r#"
/Delimiters.sol:5:23 function next(uint j) public pure returns (uint) {

"#]],
        );
    }
}

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
