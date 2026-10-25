//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// CHECK-LABEL: fn @guarded
// CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 96
// CHECK-NOT: calldataload {{v[0-9]+}}

// Without `a <= end`, the sum is not bounded and every word keeps its own load.
// CHECK-LABEL: fn @unguarded
// CHECK-NOT: calldatacopy
// CHECK: calldataload {{v[0-9]+}}
// CHECK: calldataload {{v[0-9]+}}
// CHECK: calldataload {{v[0-9]+}}

// CHECK-LABEL: fn @bounded
// CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 64

// CHECK-LABEL: fn @wrapping
// CHECK-NOT: calldatacopy
// CHECK: calldataload {{v[0-9]+}}
// CHECK: calldataload {{v[0-9]+}}
// CHECK: calldataload {{v[0-9]+}}

contract Test {
    // `x = a + b` with `a <= end`, `b <= end - a`, and room for the run behind `x`, so the
    // last word's address cannot wrap.
    function guarded() external pure {
        assembly {
            let a := calldataload(4)
            let b := calldataload(36)
            let end := calldatasize()
            if gt(a, end) { revert(0, 0) }
            if gt(b, sub(end, a)) { revert(0, 0) }
            let x := add(a, b)
            if lt(sub(end, x), 96) { revert(0, 0) }
            let p := mload(0x40)
            mstore(p, calldataload(x))
            mstore(add(p, 32), calldataload(add(x, 32)))
            mstore(add(p, 64), calldataload(add(x, 64)))
            return(p, 96)
        }
    }

    // The same without `a <= end`: `a + b` can wrap.
    function unguarded() external pure {
        assembly {
            let a := calldataload(4)
            let b := calldataload(36)
            let end := calldatasize()
            if gt(b, sub(end, a)) { revert(0, 0) }
            let x := add(a, b)
            if lt(sub(end, x), 96) { revert(0, 0) }
            let p := mload(0x40)
            mstore(p, calldataload(x))
            mstore(add(p, 32), calldataload(add(x, 32)))
            mstore(add(p, 64), calldataload(add(x, 64)))
            return(p, 96)
        }
    }

    // A source below 2^64 cannot wrap.
    function bounded() external pure {
        assembly {
            let x := calldataload(4)
            if gt(x, 0xffffffffffffffff) { revert(0, 0) }
            let p := mload(0x40)
            mstore(p, calldataload(x))
            mstore(add(p, 32), calldataload(add(x, 32)))
            return(p, 64)
        }
    }

    // A source that can reach the top of the address space wraps to the start of calldata.
    function wrapping() external pure {
        assembly {
            let x := calldataload(4)
            let p := mload(0x40)
            mstore(p, calldataload(x))
            mstore(add(p, 32), calldataload(add(x, 32)))
            mstore(add(p, 64), calldataload(add(x, 64)))
            return(p, 96)
        }
    }
}
