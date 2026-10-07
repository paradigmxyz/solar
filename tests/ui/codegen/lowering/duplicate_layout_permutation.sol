//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:

// The stack entering the outer loop holds one value twice. Gas mode searches
// such permutations exactly up to six words, so three swaps reach the loop
// layout instead of two `exchange` pseudo-ops that lower to six swaps.
// CHECK-LABEL: @module Test_runtime
// CHECK: calldatacopy
// CHECK: dup 4
// CHECK-NEXT: swap 2
// CHECK-NEXT: swap 3
// CHECK-NEXT: swap 4
// CHECK-NEXT: jump bb{{[0-9]+}}
// CHECK-NOT: exchange
contract Test {
    function sort(uint256[] memory a) external pure returns (uint256[] memory) {
        assembly {
            let n := mload(a)
            mstore(a, 0)
            let h := add(a, shl(5, n))
            for { let i := add(a, 32) } 1 {} {
                i := add(i, 32)
                if gt(i, h) { break }
                let k := mload(i)
                let j := sub(i, 32)
                let v := mload(j)
                if iszero(gt(v, k)) { continue }
                for {} 1 {} {
                    mstore(add(j, 32), v)
                    j := sub(j, 32)
                    v := mload(j)
                    if iszero(gt(v, k)) { break }
                }
                mstore(add(j, 32), k)
            }
            mstore(a, n)
        }
        return a;
    }
}
