use super::support::RequestFixture;
use snapbox::{IntoData, str};

#[test]
fn returns_parameter_and_type_hints_for_selected_callables() {
    check(
        r#"
        //- /Hints.sol
        library L {
            function add(uint256 self, uint256 amount) internal pure returns (uint256) {
                return self + amount;
            }
        }

        using L for uint256;

        contract Target {
            constructor(uint256 amount, address owner) {}
        }

        contract C {
            function target(uint256 amount, address account) public pure returns (uint256) {
                return amount;
            }

            function f(uint256 value) public pure returns (uint256) {
                return value;
            }

            function f(string memory text) public pure returns (uint256) {
                return bytes(text).length;
            }

            function pair(uint256 amount) internal pure returns (uint256, bool) {
                return (amount, amount != 0);
            }

            function sideEffect(uint256 input) public pure {
                input;
            }

            function caller(address user, uint256 value) public returns (uint256) {
                Target deployed = new Target(1, user);
                sideEffect(value.add(3));
                (uint256 a, ) = pair(1);
                return target(1, user) + target({amount: 2, account: user}) + f("abc") + a;
            }
        }
        "#,
        str![[r#"
26:37 PARAMETER amount:
26:40 PARAMETER owner:
26:45 TYPE : contract Target
27:19 PARAMETER input:
27:29 PARAMETER amount:
27:31 TYPE : uint256
28:29 PARAMETER amount:
28:31 TYPE : (uint256, bool)
29:22 PARAMETER amount:
29:25 PARAMETER account:
29:30 TYPE : uint256
29:67 TYPE : uint256
29:72 PARAMETER text:
29:78 TYPE : uint256

"#]],
    );
}

#[test]
fn skips_type_hints_for_casts_builtins_and_inline_assembly() {
    check(
        r#"
        //- /Hints.sol
        type MyUdvt is uint256;
        contract Target {}
        enum SomeEnum { A, B }

        contract C {
            uint256[] xs;

            function value() public pure returns (uint256) {
                return 1;
            }

            function run(address addr, uint256 x, MyUdvt y) public returns (uint256) {
                xs.push(1);
                xs.pop();
                MyUdvt wrapped = MyUdvt.wrap(x);
                uint256 unwrapped = MyUdvt.unwrap(y);
                Target t = Target(addr);
                SomeEnum e = SomeEnum(0);
                uint256 n = uint256(1);
                uint256 v = value();
                assembly {
                    let z := add(1, 2)
                }
                return MyUdvt.unwrap(wrapped) + unwrapped;
            }
        }
        "#,
        str![[r#"
11:39 TYPE : MyUdvt
12:44 TYPE : uint256
16:27 TYPE : uint256
20:37 TYPE : uint256

"#]],
    );
}

#[test]
fn skips_parameter_hints_for_arguments_with_matching_names() {
    check(
        r#"
        //- /Hints.sol
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
        str![[r#"
6:50 TYPE : uint256
7:36 PARAMETER amount:
7:47 TYPE : uint256
11:19 PARAMETER code:

"#]],
    );
}

#[test]
fn returns_parameter_hints_for_solidity_callable_forms() {
    check(
        r#"
        //- /Hints.sol
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
        str![[r#"
6:23 PARAMETER baseValue:
13:27 PARAMETER ctorValue:
14:43 PARAMETER requiredValue:
15:32 PARAMETER left:
15:35 PARAMETER right:
16:18 PARAMETER id:
16:21 PARAMETER account:
17:19 PARAMETER code:
17:22 PARAMETER account:

"#]],
    );
}

#[test]
fn uses_function_type_parameter_names_for_variable_and_struct_field_calls() {
    check(
        r#"
        //- /Hints.sol
        contract C {
            struct Holder {
                function(uint256 amount, address account) internal returns (uint256) callback;
            }

            Holder holder;

            function target(uint256 amount, address account) internal pure returns (uint256) {
                return amount;
            }

            function caller(address user) public returns (uint256) {
                function(uint256 amount, address account) internal pure returns (uint256) f = target;
                return f(1, user) + holder.callback(1, user);
            }
        }
        "#,
        str![[r#"
10:17 PARAMETER amount:
10:20 PARAMETER account:
10:25 TYPE : uint256
10:44 PARAMETER amount:
10:47 PARAMETER account:
10:52 TYPE : uint256

"#]],
    );
}

#[test]
fn prefers_selected_attached_function_over_colliding_struct_field() {
    check(
        r#"
        //- /Hints.sol
        struct Holder {
            function(bool fieldFlag, address fieldAccount) internal pure callback;
        }

        library L {
            function callback(
                Holder memory self,
                uint256 attachedFirst,
                address attachedAccount
            ) internal pure {
                self;
                attachedFirst;
                attachedAccount;
            }
        }

        contract C {
            using L for Holder;

            function fieldTarget(bool fieldFlag, address fieldAccount) internal pure {
                fieldFlag;
                fieldAccount;
            }

            function caller(address user) public pure {
                Holder memory holder = Holder({callback: fieldTarget});
                holder.callback(1, user);
            }
        }
        "#,
        str![[r#"
22:24 PARAMETER attachedFirst:
22:27 PARAMETER attachedAccount:

"#]],
    );
}

#[test]
fn uses_target_parameter_names_for_abi_encode_call_arguments() {
    check(
        r#"
        //- /Hints.sol
        interface I {
            function target(uint256 amount, address account) external returns (uint256);
            function single(uint256 amount) external returns (uint256);
        }

        contract C {
            function caller(address user) public pure {
                abi.encodeCall(I.target, (1, user));
                abi.encodeCall(I.single, 1);
                abi.encodeCall(I.target, (1, user, 3));
                abi.encodeCall(I.target, (, user));
            }
        }
        "#,
        str![[r#"
6:34 PARAMETER amount:
6:37 PARAMETER account:
6:43 TYPE : bytes memory
7:33 PARAMETER amount:
7:35 TYPE : bytes memory
8:46 TYPE : bytes memory
9:42 TYPE : bytes memory

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
5:22 PARAMETER first:
5:25 PARAMETER second:
5:27 TYPE : uint256

"#]],
    );
    // Files without stored hints, such as ones outside the analysis, return no hints.
    fixture.check_inlay_hints("/Unanalyzed.sol", "");
}

/// Checks all hints in `/Hints.sol`. The exact snapshot also pins error-recovery behavior.
fn check(fixture: &str, expected: impl IntoData) {
    RequestFixture::new_allowing_diagnostics(fixture, "/Hints.sol")
        .check_inlay_hints("/Hints.sol", expected);
}
