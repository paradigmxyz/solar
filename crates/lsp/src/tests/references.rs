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
fn indexes_function_and_state_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Symbols.sol
        contract C {
            uint256 $2stateValue;

            function $1target(uint256 input) public view returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = localValue;
            }

            function caller() public view {
                uint256 callerLocal = target(stateValue);
            }
        }
        "#,
        "/Symbols.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Symbols.sol:2:13 function target(uint256 input) public view returns (uint256 output) {
/Symbols.sol:7:30 uint256 callerLocal = target(stateValue);

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Symbols.sol:1:12 uint256 stateValue;
/Symbols.sol:3:37 uint256 localValue = input + stateValue;
/Symbols.sol:7:37 uint256 callerLocal = target(stateValue);

"#]],
    );
}

#[test]
fn indexes_member_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Members.sol
        contract C {
            enum Choice { A, B }
            struct Data { uint256 $1field; }

            function read(Data memory data) public pure returns (uint256) {
                Choice choice = Choice.A;
                return data.field;
            }
        }
        "#,
        "/Members.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Members.sol:2:26 struct Data { uint256 field; }
/Members.sol:5:20 return data.field;

"#]],
    );
}

#[test]
fn distinguishes_enum_and_variant_declarations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Enum.sol
        contract C {
            enum $1Choice { $2A, B }

            function read() public pure returns (Choice) {
                return Choice.A;
            }
        }
        "#,
        "/Enum.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Enum.sol:1:9 enum Choice { A, B }
/Enum.sol:2:41 function read() public pure returns (Choice) {
/Enum.sol:3:15 return Choice.A;

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Enum.sol:1:18 enum Choice { A, B }
/Enum.sol:3:22 return Choice.A;

"#]],
    );
}

#[test]
fn skips_generated_getter_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Getter.sol
        contract C {
            uint256 public $1x;

            function read() external view returns (uint256) {
                return x;
            }
        }
        "#,
        "/Getter.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Getter.sol:1:19 uint256 public x;
/Getter.sol:3:15 return x;

"#]],
    );
}

#[test]
fn indexes_selected_overload_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overload.sol
        contract C {
            function $1f(uint256) public {}
            function $2f(string memory) public {}
            function g() public {
                f(uint256(1));
            }
        }
        "#,
        "/Overload.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Overload.sol:1:13 function f(uint256) public {}
/Overload.sol:4:8 f(uint256(1));

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/Overload.sol:2:13 function f(string memory) public {}

"#]],
    );
}

#[test]
fn indexes_using_directive_references() {
    let fixture = RequestFixture::new(
        r#"
        //- /Using.sol
        library $1L {
            function inc(uint256 value) internal pure returns (uint256) {
                return value + 1;
            }
        }

        using L for uint256;

        contract C {
            function f(uint256 value) public pure returns (uint256) {
                return value.inc();
            }
        }
        "#,
        "/Using.sol",
    );

    fixture.check_references(
        "$1",
        true,
        str![[r#"
/Using.sol:0:8 library L {
/Using.sol:5:6 using L for uint256;

"#]],
    );
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

#[test]
fn inherited_natspec_keeps_original_parameter_target() {
    let fixture = RequestFixture::new(
        r#"
        //- /NatSpec.sol
        contract Base {
            /// @param $1amount The amount.
            function f(uint amount) public virtual {}
        }
        contract Child is Base {
            function f(uint $2value) public override {}
        }
        "#,
        "/NatSpec.sol",
    );
    fixture.check_references(
        "$1",
        true,
        str![[r#"
/NatSpec.sol:1:15 /// @param amount The amount.
/NatSpec.sol:2:20 function f(uint amount) public virtual {}

"#]],
    );
    fixture.check_references(
        "$2",
        true,
        str![[r#"
/NatSpec.sol:5:20 function f(uint value) public override {}

"#]],
    );
}
