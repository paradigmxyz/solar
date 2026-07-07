use super::support::RequestFixture;
use snapbox::str;

#[test]
fn returns_parameter_hints_for_positional_calls_and_skips_named_args() {
    let fixture = RequestFixture::new(
        r#"
        //- /Hints.sol
        contract C {
            function target(uint256 amount, address account) public pure returns (uint256) {
                return amount;
            }

            function caller(address user) public pure returns (uint256) {
                return target(1, user) + target({amount: 2, account: user});
            }
        }
        "#,
        "/Hints.sol",
    );

    fixture.check_inlay_hints(
        "/Hints.sol",
        str![[r#"
PARAMETER amount:
PARAMETER account:
TYPE : uint256
TYPE : uint256

"#]],
    );
}

#[test]
fn uses_selected_overload_for_parameter_hints() {
    let fixture = RequestFixture::new(
        r#"
        //- /Overload.sol
        contract C {
            function f(uint256 value) public pure returns (uint256) {
                return value;
            }

            function f(string memory text) public pure returns (uint256) {
                return bytes(text).length;
            }

            function caller() public pure returns (uint256) {
                return f("abc");
            }
        }
        "#,
        "/Overload.sol",
    );

    fixture.check_inlay_hints(
        "/Overload.sol",
        str![[r#"
PARAMETER text:
TYPE : uint256

"#]],
    );
}

#[test]
fn skips_attached_using_receiver_for_parameter_hints() {
    let fixture = RequestFixture::new(
        r#"
        //- /Using.sol
        library L {
            function add(uint256 self, uint256 amount) internal pure returns (uint256) {
                return self + amount;
            }
        }

        using L for uint256;

        contract C {
            function caller(uint256 value) public pure returns (uint256) {
                return value.add(3);
            }
        }
        "#,
        "/Using.sol",
    );

    fixture.check_inlay_hints(
        "/Using.sol",
        str![[r#"
PARAMETER amount:
TYPE : uint256

"#]],
    );
}

#[test]
fn skips_parameter_hints_for_arguments_with_matching_names() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Names.sol
        contract C {
            error Bad(uint256 code, address account);

            function target(uint256 amount, address account) public pure returns (uint256) {
                return amount;
            }

            function caller(address account, uint256 amount) public pure returns (uint256) {
                uint256 bothSame = target(amount, account);
                uint256 secondSame = target(1, account);
                return bothSame + secondSame;
            }

            function fail(address user) public pure {
                revert Bad(7, account);
            }
        }
        "#,
        "/Names.sol",
    );

    fixture.check_inlay_hints(
        "/Names.sol",
        str![[r#"
TYPE : uint256
PARAMETER amount:
TYPE : uint256
PARAMETER code:

"#]],
    );
}

#[test]
fn handles_contextual_builtin_callees() {
    let fixture = RequestFixture::new(
        r#"
        //- /Builtins.sol
        type MyUdvt is uint256;

        contract C {
            uint256[] xs;

            function run(uint256 x, MyUdvt y) public returns (uint256) {
                xs.push(1);
                xs.pop();

                MyUdvt wrapped = MyUdvt.wrap(x);
                uint256 unwrapped = MyUdvt.unwrap(y);

                return MyUdvt.unwrap(wrapped) + unwrapped;
            }
        }
        "#,
        "/Builtins.sol",
    );

    fixture.check_inlay_hints(
        "/Builtins.sol",
        str![[r#"
TYPE : MyUdvt
TYPE : uint256
TYPE : uint256

"#]],
    );
}

#[test]
fn skips_inlay_hints_inside_inline_assembly() {
    let fixture = RequestFixture::new(
        r#"
        //- /Assembly.sol
        contract C {
            function run() public pure {
                assembly {
                    let y := add(1, 2)
                }
            }
        }
        "#,
        "/Assembly.sol",
    );

    fixture.check_inlay_hints(
        "/Assembly.sol",
        str![[r#"
"#]],
    );
}

#[test]
fn skips_type_hints_for_explicit_casts() {
    let fixture = RequestFixture::new(
        r#"
        //- /Casts.sol
        contract Target {}

        enum SomeEnum { A, B }

        contract Repro {
            function value() public pure returns (uint256) {
                return 1;
            }

            function run(address addr) public pure returns (SomeEnum) {
                Target t = Target(addr);
                SomeEnum e = SomeEnum(0);
                uint256 n = uint256(1);
                uint256 x = value();
                return e;
            }
        }
        "#,
        "/Casts.sol",
    );

    fixture.check_inlay_hints(
        "/Casts.sol",
        str![[r#"
TYPE : uint256

"#]],
    );
}

#[test]
fn returns_parameter_hints_for_solidity_callable_forms() {
    let fixture = RequestFixture::new(
        r#"
        //- /Forms.sol
        contract BaseList {
            constructor(uint256 baseValue) {}
        }

        contract BaseCtor {
            constructor(uint256 ctorValue) {}
        }

        contract C is BaseList(1), BaseCtor {
            struct Pair { uint256 left; uint256 right; }
            event Seen(uint256 indexed id, address account);
            error Bad(uint256 code, address account);

            modifier only(uint256 requiredValue) {
                _;
            }

            constructor() BaseCtor(2) {}

            function run(address user) public only(3) {
                Pair memory pair = Pair(4, 5);
                emit Seen(6, user);
                revert Bad(7, user);
            }
        }
        "#,
        "/Forms.sol",
    );

    fixture.check_inlay_hints(
        "/Forms.sol",
        str![[r#"
PARAMETER baseValue:
PARAMETER ctorValue:
PARAMETER requiredValue:
PARAMETER left:
PARAMETER right:
PARAMETER id:
PARAMETER account:
PARAMETER code:
PARAMETER account:

"#]],
    );
}

#[test]
fn returns_parameter_hints_for_new_contract_constructor_calls() {
    let fixture = RequestFixture::new(
        r#"
        //- /New.sol
        contract Target {
            constructor(uint256 amount, address owner) {}
        }

        contract C {
            function run(address user) public {
                Target deployed = new Target(1, user);
            }
        }
        "#,
        "/New.sol",
    );

    fixture.check_inlay_hints(
        "/New.sol",
        str![[r#"
PARAMETER amount:
PARAMETER owner:
TYPE : contract Target

"#]],
    );
}

#[test]
fn returns_call_type_hints_for_non_unit_calls() {
    let fixture = RequestFixture::new(
        r#"
        //- /Types.sol
        contract C {
            function value() public pure returns (uint256) {
                return 1;
            }

            function sideEffect(uint256 input) public pure {
                input;
            }

            function caller() public pure {
                uint256 x = value();
                sideEffect(x);
            }
        }
        "#,
        "/Types.sol",
    );

    fixture.check_inlay_hints(
        "/Types.sol",
        str![[r#"
TYPE : uint256
PARAMETER input:

"#]],
    );
}

#[test]
fn displays_multi_return_call_type_hints_as_solidity_tuples() {
    let fixture = RequestFixture::new(
        r#"
        //- /MultiReturn.sol
        contract C {
            function pair(uint256 amount) internal pure returns (uint256, bool) {
                return (amount, amount != 0);
            }

            function caller() public pure returns (uint256, bool) {
                return pair(1);
            }
        }
        "#,
        "/MultiReturn.sol",
    );

    fixture.check_inlay_hints(
        "/MultiReturn.sol",
        str![[r#"
PARAMETER amount:
TYPE : (uint256, bool)

"#]],
    );
}

#[test]
fn uses_function_type_parameter_names_for_function_variable_calls() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /FunctionType.sol
        contract C {
            function target(uint256 amount, address account) internal pure returns (uint256) {
                return amount;
            }

            function caller(address user) public pure returns (uint256) {
                function(uint256 amount, address account) internal pure returns (uint256) f = target;
                return f(1, user);
            }
        }
        "#,
        "/FunctionType.sol",
    );

    fixture.check_inlay_hints(
        "/FunctionType.sol",
        str![[r#"
PARAMETER amount:
PARAMETER account:
TYPE : uint256

"#]],
    );
}

#[test]
fn uses_function_type_parameter_names_for_struct_field_calls() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /FunctionField.sol
        contract C {
            struct Holder {
                function(uint256 amount, address account) internal returns (uint256) callback;
            }

            Holder holder;

            function caller(address user) public returns (uint256) {
                return holder.callback(1, user);
            }
        }
        "#,
        "/FunctionField.sol",
    );

    fixture.check_inlay_hints(
        "/FunctionField.sol",
        str![[r#"
PARAMETER amount:
PARAMETER account:
TYPE : uint256

"#]],
    );
}

#[test]
fn uses_target_parameter_names_for_abi_encode_call_tuple() {
    let fixture = RequestFixture::new(
        r#"
        //- /AbiEncodeCall.sol
        interface I {
            function target(uint256 amount, address account) external returns (uint256);
        }

        contract C {
            function caller(address user) public pure returns (bytes memory) {
                return abi.encodeCall(I.target, (1, user));
            }
        }
        "#,
        "/AbiEncodeCall.sol",
    );

    fixture.check_inlay_hints(
        "/AbiEncodeCall.sol",
        str![[r#"
PARAMETER amount:
PARAMETER account:
TYPE : bytes memory

"#]],
    );
}

#[test]
fn uses_target_parameter_names_for_single_abi_encode_call_argument() {
    let fixture = RequestFixture::new(
        r#"
        //- /AbiEncodeCallSingle.sol
        interface I {
            function target(uint256 amount) external returns (uint256);
        }

        contract C {
            function caller() public pure returns (bytes memory) {
                return abi.encodeCall(I.target, 1);
            }
        }
        "#,
        "/AbiEncodeCallSingle.sol",
    );

    fixture.check_inlay_hints(
        "/AbiEncodeCallSingle.sol",
        str![[r#"
PARAMETER amount:
TYPE : bytes memory

"#]],
    );
}

#[test]
fn skips_abi_encode_call_parameter_hints_for_tuple_holes() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /AbiEncodeCallHole.sol
        interface I {
            function target(uint256 amount, address account) external returns (uint256);
        }

        contract C {
            function caller(address user) public pure returns (bytes memory) {
                return abi.encodeCall(I.target, (, user));
            }
        }
        "#,
        "/AbiEncodeCallHole.sol",
    );

    fixture.check_inlay_hints(
        "/AbiEncodeCallHole.sol",
        str![[r#"
TYPE : bytes memory

"#]],
    );
}

#[test]
fn filters_hints_by_requested_range() {
    let fixture = RequestFixture::new(
        r#"
        //- /Range.sol
        contract C {
            function f(uint256 first, uint256 second) public pure returns (uint256) {
                return first + second;
            }

            function caller() public pure returns (uint256) {
                $1uint256 a = f(1, 2);
                $2uint256 b = f(3, 4);
                return a + b;
            }
        }
        "#,
        "/Range.sol",
    );

    fixture.check_inlay_hints_between(
        "$1",
        "$2",
        str![[r#"
PARAMETER first:
PARAMETER second:
TYPE : uint256

"#]],
    );
}
