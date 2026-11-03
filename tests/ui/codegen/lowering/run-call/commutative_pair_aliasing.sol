//@ codegen-matrix: standard
//@ run-call: modulusAlias1 7, 9 => 2
//@ run-call: modulusAlias2 7, 9 => 4
//@ run-call: modulusAlias3 7, 9 => 5
//@ run-call: modulusAlias1 0x100000000000000000000000000000000000000000000000063, 0xf4243 => 1000003
//@ run-call: modulusAlias2 0x100000000000000000000000000000000000000000000000063, 0xf4243 => 1000006000009
//@ run-call: modulusAlias3 0x100000000000000000000000000000000000000000000000063, 0xf4243 => 947579
//@ run-call: modulusAlias1 0xf4243, 0x8000000000000000000000000000000000000000000000000000000000000003 => 578329
//@ run-call: modulusAlias2 0xf4243, 0x8000000000000000000000000000000000000000000000000000000000000003 => 428852
//@ run-call: modulusAlias3 0xf4243, 0x8000000000000000000000000000000000000000000000000000000000000003 => 2000006
//@ run-call: loop 7, 9, 10 => 0
//@ run-call: loop 0x8000000000000000000000000000000000000000000000000000000000003039, 0x100000000000000000000000000000000000000000000000063, 0xf4243 => 726561

// The planner may push the commutative pair of `addmod` and `mulmod` in either order, so the
// modulus must stay in place whether it aliases an operand, both operands are one value, or the
// operands arrive from a loop.
contract CommutativePairAliasing {
    function modulusAlias1(uint256 a, uint256 b) external pure returns (uint256 r) {
        assembly {
            r := addmod(a, b, a)
        }
    }

    function modulusAlias2(uint256 a, uint256 b) external pure returns (uint256 r) {
        assembly {
            r := mulmod(b, b, a)
        }
    }

    function modulusAlias3(uint256 a, uint256 b) external pure returns (uint256 r) {
        assembly {
            r := addmod(a, a, b)
        }
    }

    function loop(uint256 a, uint256 b, uint256 n) external pure returns (uint256 r) {
        assembly {
            r := a
            for { let i := 0 } lt(i, 5) { i := add(i, 1) } {
                r := addmod(mulmod(r, b, n), a, n)
            }
        }
    }
}
