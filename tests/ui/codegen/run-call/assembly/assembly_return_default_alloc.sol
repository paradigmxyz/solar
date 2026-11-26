//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: mul 2 => 0x030644e72e131a029b85045b68181585d97816a916871ca8d3c208c16d87cfd3, 0x15ed738c0e0a7c92e7845f96b2ae9c0a68a6a449e3538fc7ff3ebf7a5a18a2c4
//@ run-call: named => 5, 6
//@ run-call: mixed => 5, 6

// Assembly can use memory at the free memory pointer as temporary space. The default object of a
// memory return must exist before the block runs, so allocating it inside the block cannot
// overwrite the block's data.
contract AssemblyReturnDefaultAlloc {
    struct Point {
        uint256 x;
        uint256 y;
    }

    struct Mixed {
        uint256 x;
        bytes b;
    }

    // CHECK-LABEL: fn @ecMul
    // CHECK: mload 64
    // CHECK-NOT: alloc
    // CHECK: ret
    function ecMul(uint256 k, Point memory p) internal view returns (Point memory) {
        Point memory result;
        assembly ("memory-safe") {
            let free := mload(0x40)
            mstore(free, mload(p))
            mstore(add(free, 0x20), mload(add(p, 0x20)))
            mstore(add(free, 0x40), k)
            let ok := staticcall(gas(), 0x07, free, 0x60, free, 0x40)
            if iszero(ok) { revert(0, 0) }
            mstore(result, mload(free))
            mstore(add(result, 0x20), mload(add(free, 0x20)))
        }
        return result;
    }

    // CHECK-LABEL: fn @pair
    // CHECK: alloc 64
    // CHECK: mload 64
    function pair() internal pure returns (Point memory r) {
        assembly ("memory-safe") {
            let free := mload(0x40)
            mstore(free, 5)
            mstore(add(free, 0x20), 6)
            if iszero(free) { revert(0, 0) }
            mstore(r, mload(free))
            mstore(add(r, 0x20), mload(add(free, 0x20)))
        }
    }

    function mixedStruct() internal pure returns (Mixed memory r) {
        assembly ("memory-safe") {
            let free := mload(0x40)
            mstore(free, 5)
            mstore(add(free, 0x20), 6)
            if iszero(free) { revert(0, 0) }
            mstore(r, mload(free))
        }
    }

    function mul(uint256 k) external view returns (uint256, uint256) {
        Point memory r = ecMul(k, Point(1, 2));
        return (r.x, r.y);
    }

    function named() external pure returns (uint256, uint256) {
        Point memory r = pair();
        return (r.x, r.y);
    }

    function mixed() external pure returns (uint256, uint256) {
        Mixed memory r = mixedStruct();
        return (r.x, r.b.length + 6);
    }
}
