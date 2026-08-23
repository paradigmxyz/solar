//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: direct() => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f
//@ run-call: shortDirect() => 0x1234000000000000000000000000000000000000000000000000000000000000
//@ run-call: local() => 0x202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f
//@ run-call: stringDirect() => 0x68656c6c6f000000000000000000000000000000000000000000000000000000
//@ run-call: builtin() => 0x1234000000000000000000000000000000000000000000000000000000000000
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
}
