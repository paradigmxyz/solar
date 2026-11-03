//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: direct => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@ run-call: shortDirect => 0x1234000000000000000000000000000000000000000000000000000000000000
//@ run-call: local => 0x202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f
//@ run-call: stringDirect => 0x68656c6c6f000000000000000000000000000000000000000000000000000000
//@ run-call: builtin => 0x1234000000000000000000000000000000000000000000000000000000000000
//@ run-call: loop => 3
//@ run-call: userArg => 0x626f6f6d00000000000000000000000000000000000000000000000000000000
//@ run-call: viaLet => 0x626f6f6d00000000000000000000000000000000000000000000000000000000
//@ run-call: userHexArg => 0xcafe000000000000000000000000000000000000000000000000000000000000
//@ run-call: secondArg => 0x626f6f6d00000000000000000000000000000000000000000000000000000000
//@ run-call: nestedCall => 0x626f6f6d00000000000000000000000000000000000000000000000000000000
//@ run-call: emptyStringArg => 0
//@ run-call: emptyHexArg => 0
//@ run-call: full32Arg => 0x3031323334353637383961626364656630313233343536373839616263646566
//@ run-call: escapesArg => 0x410a095c22c3a9007a0000000000000000000000000000000000000000000000
//@ run-call: multiAssign => 0x7100000000000000000000000000000000000000000000000000000000000000, 0x7000000000000000000000000000000000000000000000000000000000000000
//@ run-call: multiLet => 0x7100000000000000000000000000000000000000000000000000000000000000, 0x7000000000000000000000000000000000000000000000000000000000000000
//@ run-call: voidCall => 0x6162000000000000000000000000000000000000000000000000000000000000
//@ run-call: switchUserCall => 1
//@ run-call: ifEmptyString => 1
//@ run-call: ifNonZeroHex => 2
//@ run-call: ifZeroHex => 1
//@ run-call: ifUserCall => 1
//@ run-call: forEmptyString => 0
//@ run-call: errorString => 0x08c379a000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000004626f6f6d00000000000000000000000000000000000000000000000000000000
// ported-from: test/libsolidity/semanticTests/literals/hex_string_with_non_printable_characters.sol
contract C {
    function direct() external pure returns (bytes32 result) {
        assembly {
            result := hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
        }
    }

    function shortDirect() external pure returns (bytes32 result) {
        assembly {
            result := hex"1234"
        }
    }

    function local() external pure returns (bytes32 result) {
        assembly {
            let value := hex"202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f"
            result := value
        }
    }

    function stringDirect() external pure returns (bytes32 result) {
        assembly {
            result := "hello"
        }
    }

    function builtin() external pure returns (bytes32 result) {
        assembly {
            result := or(hex"1234", 0)
        }
    }

    function loop() external pure returns (uint result) {
        assembly {
            if false { result := 99 }
            for {} true {} {
                result := add(result, 1)
                if eq(result, 3) { break }
            }
        }
    }

    function userArg() external pure returns (bytes32 r) {
        assembly {
            function id(x) -> y { y := x }
            r := id("boom")
        }
    }

    function viaLet() external pure returns (bytes32 r) {
        assembly {
            function id(x) -> y { y := x }
            let s := "boom"
            r := id(s)
        }
    }

    function userHexArg() external pure returns (bytes32 r) {
        assembly {
            function id(x) -> y { y := x }
            r := id(hex"cafe")
        }
    }

    function secondArg() external pure returns (bytes32 r) {
        assembly {
            function second(a, b) -> y { y := b }
            r := second(1, "boom")
        }
    }

    function nestedCall() external pure returns (bytes32 r) {
        assembly {
            function id(x) -> y { y := x }
            r := id(id("boom"))
        }
    }

    function emptyStringArg() external pure returns (uint r) {
        assembly {
            function id(x) -> y { y := x }
            r := id("")
        }
    }

    function emptyHexArg() external pure returns (uint r) {
        assembly {
            function id(x) -> y { y := x }
            r := id(hex"")
        }
    }

    function full32Arg() external pure returns (bytes32 r) {
        assembly {
            function id(x) -> y { y := x }
            r := id("0123456789abcdef0123456789abcdef")
        }
    }

    function escapesArg() external pure returns (bytes32 r) {
        assembly {
            function id(x) -> y { y := x }
            r := id("\x41\n\t\\\"\u00e9\x00z")
        }
    }

    function multiAssign() external pure returns (bytes32 p, bytes32 q) {
        assembly {
            function two(a, b) -> x, y { x := b y := a }
            p, q := two("p", hex"71")
        }
    }

    function multiLet() external pure returns (bytes32 p, bytes32 q) {
        assembly {
            function two(a, b) -> x, y { x := b y := a }
            let s, t := two("p", hex"71")
            p := s
            q := t
        }
    }

    function voidCall() external returns (bytes32 r) {
        assembly {
            function st(x) { sstore(0, x) }
            st("ab")
            r := sload(0)
        }
    }

    function switchUserCall() external pure returns (uint r) {
        assembly {
            function id(x) -> y { y := x }
            switch id("ab")
            case "ab" { r := 1 }
            default { r := 2 }
        }
    }

    function ifEmptyString() external pure returns (uint r) {
        assembly {
            r := 1
            if "" { r := 2 }
        }
    }

    function ifNonZeroHex() external pure returns (uint r) {
        assembly {
            r := 1
            if hex"01" { r := 2 }
        }
    }

    function ifZeroHex() external pure returns (uint r) {
        assembly {
            r := 1
            if hex"00" { r := 2 }
        }
    }

    function ifUserCall() external pure returns (uint r) {
        assembly {
            function id(x) -> y { y := x }
            r := 1
            if id("") { r := 2 }
        }
    }

    function forEmptyString() external pure returns (uint r) {
        assembly {
            for {} "" {} { r := 5 break }
        }
    }

    function errorString() external pure returns (bytes memory out) {
        assembly {
            function revertString(p, message, length) {
                mstore(p, shl(224, 0x08c379a0))
                mstore(add(p, 4), 0x20)
                mstore(add(p, 36), length)
                mstore(add(p, 68), message)
            }
            out := mload(0x40)
            mstore(out, 100)
            revertString(add(out, 32), "boom", 4)
            mstore(0x40, add(out, 160))
        }
    }
}
