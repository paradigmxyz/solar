//@ codegen-matrix: standard disasm
//@[disasm] compile-flags: -Ogas -Zdump=disasm-runtime
//@[disasm] filecheck:
//@ run-call: sorted => 1, 2, 3, 4
//@ run-call: previous 1 => 1
//@ run-call: previous 2 => 1
//@ run-call: previous 32 => 1
//@ run-call: wrapping 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: wrapping 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

// The inner loop of `sorted` steps `j` down by one slot and uses the old `j` only after it. The
// stack carries the stepped word through the loop and the exit computes the old one from it,
// instead of the backedge dropping the old word on every iteration.
// CHECK-LABEL: // === {{.*}}:C (runtime) ===
// CHECK: {{JUMPI[[:space:]]+POP[[:space:]]+PUSH1 0x20[[:space:]]+ADD[[:space:]]+MSTORE}}
contract C {
    function sorted() external pure returns (uint256, uint256, uint256, uint256) {
        uint256[4] memory a = [uint256(4), 3, 2, 1];
        assembly {
            let h := add(a, 96)
            let w := not(31)
            for { let i := add(a, 0x20) } iszero(gt(i, h)) { i := add(i, 0x20) } {
                let k := mload(i)
                let j := add(i, w)
                let v := mload(j)
                if iszero(gt(v, k)) { continue }
                for {} 1 {} {
                    mstore(add(j, 0x20), v)
                    j := add(j, w)
                    v := mload(j)
                    if iszero(gt(v, k)) { break }
                }
                mstore(add(j, 0x20), k)
            }
        }
        return (a[0], a[1], a[2], a[3]);
    }

    function previous(uint256 n) external pure returns (uint256 last) {
        assembly {
            for {} 1 {} {
                last := n
                n := sub(n, 1)
                if iszero(n) { break }
            }
        }
    }

    function wrapping(uint256 n) external pure returns (uint256 last) {
        assembly {
            for {} 1 {} {
                last := n
                n := add(n, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
                if eq(n, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe) { break }
            }
        }
    }
}
