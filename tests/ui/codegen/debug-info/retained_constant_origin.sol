//@ compile-flags: -Ogas --emit=srcmap -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `P` is pushed once and kept on the stack for the three modular operations, which all run in the
// block of the push. The push is shared code that runs whenever the readers do, so it takes the
// merged location of those readers, one span per reader.

// CHECK-LABEL: ; function f(uint256 x, uint256 y)
// CHECK: push 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff !metadata(spans=[{{[0-9]+}}..{{[0-9]+}}, {{[0-9]+}}..{{[0-9]+}}, {{[0-9]+}}..{{[0-9]+}}])
contract Merged {
    uint256 constant P = 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff;

    function f(uint256 x, uint256 y) external pure returns (uint256) {
        return addmod(mulmod(x, y, P), mulmod(x, x, P), P);
    }
}

// The mask `M` is pushed once before the `if`, for readers in both arms, so it runs even when an
// arm's readers do not. It takes the origin of the code it precedes, here the branch on `c`, and
// not the location of a reader.

// CHECK-LABEL: ; function g(bool c, uint256 a, uint256 b)
// CHECK: push 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0 !metadata(span=[[#S:]]..[[#E:]])
// CHECK: jumpi !metadata(span=[[#S]]..[[#E]])
contract Hoisted {
    uint256 constant M = 0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0;

    function g(bool c, uint256 a, uint256 b) external pure returns (uint256 r) {
        unchecked {
            if (c) {
                r = (a & M) + (b & M) + (a ^ M);
            } else {
                r = (b & M) + (a & M) + (b ^ M);
            }
        }
    }
}
