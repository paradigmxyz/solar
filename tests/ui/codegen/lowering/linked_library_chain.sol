//@compile-flags: -Zcodegen --libraries Inner=0x1000000000000000000000000000000000000001,Outer=0x1000000000000000000000000000000000000002 -Zdump=evm-ir-runtime
//@ filecheck:

// A linked library may itself call another linked library, forwarding a
// storage-reference parameter: the storage struct travels as its slot through
// BOTH delegatecall hops and still addresses the original caller's storage.
// `C.go` delegatecalls `Outer.callThrough`, which delegatecalls `Inner.bump`.
// Runtime behavior (return values and the mutation of `C`'s storage) is
// verified equal to solc 0.8.30's linked flow separately.

library DataTypes {
    struct Map {
        uint256 data;
    }
}

library Inner {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0xfaf4836c
    // CHECK: sload
    // CHECK: sstore
    // CHECK: return
    function bump(DataTypes.Map storage m, uint256 by) public returns (uint256) {
        m.data += by;
        return m.data;
    }
}

library Outer {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0x5e0b1cef
    // CHECK: push 0xfaf4836c
    // CHECK: push 0x1000000000000000000000000000000000000001
    // CHECK: delegatecall
    // CHECK: returndatacopy
    // CHECK: revert
    function callThrough(DataTypes.Map storage m, uint256 by) public returns (uint256) {
        if (by == 0) {
            return 0;
        }
        return Inner.bump(m, by);
    }
}

contract C {
    DataTypes.Map map;

    // CHECK-LABEL: @module runtime
    // CHECK: push 0xb20e7344
    // CHECK: push 0x5e0b1cef
    // CHECK: push 0x1000000000000000000000000000000000000002
    // CHECK: delegatecall
    // CHECK: returndatacopy
    // CHECK: revert
    function go(uint256 by) external returns (uint256) {
        return Outer.callThrough(map, by);
    }
}
