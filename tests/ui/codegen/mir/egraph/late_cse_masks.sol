//@ codegen-matrix: standard optimized optimizedSize
//@[optimized] compile-flags: -Ogas -Zdump=mir
//@[optimizedSize] compile-flags: -Osize -Zdump=mir
//@[optimized] filecheck:
//@[optimizedSize] filecheck:
//@ run-call: first 0x0000000000000000000000000000000000001234, 7 => true
//@ run-call: second 0x0000000000000000000000000000000000001234, 7 => true
//@ run-call: total => 0

// Late CSE forwards the stored address to the ABI encoder's load, leaving a
// mask on the argument. The egraph right after it removes the mask while
// integer types exist, before `lower-integers` erases them.
contract LateCseMasks {
    struct Entry {
        address owner;
        uint256 amount;
    }

    uint256 public total;

    // CHECK-LABEL: fn @digest
    // CHECK: mstore {{v[0-9]+}}, arg0
    // CHECK-NOT: = and
    // CHECK: mstore [[BUF:v[0-9]+]], arg0
    // CHECK-NOT: = and
    // CHECK: keccak256 [[BUF]], 64
    function digest(address owner, uint256 amount) internal returns (bytes32) {
        Entry memory entry = Entry(owner, amount);
        bytes32 hash = keccak256(abi.encode(entry));
        total += amount;
        return hash;
    }

    function expected(address owner, uint256 amount) internal pure returns (bytes32) {
        return keccak256(abi.encode(uint256(uint160(owner)), amount));
    }

    function first(address owner, uint256 amount) external returns (bool) {
        return digest(owner, amount) == expected(owner, amount);
    }

    function second(address owner, uint256 amount) external returns (bool) {
        return digest(owner, amount + 1) == expected(owner, amount + 1);
    }
}
