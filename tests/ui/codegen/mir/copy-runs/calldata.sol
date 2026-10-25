//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// Three adjacent words from constant calldata offsets are one copy. The copy zero-fills past the
// end of calldata, as the loads did, which the short calldata calls above check.
// CHECK-LABEL: fn @run
// CHECK: calldatacopy {{v[0-9]+}}, 4, 96
// CHECK-NOT: calldataload

// A store between the loads and stores may observe the first word, so the words stay apart.
// CHECK-LABEL: fn @between
// CHECK-NOT: calldatacopy
// CHECK: calldataload 4
// CHECK: mstore 0, 1
// CHECK: calldataload 36

// Source offsets that skip a word are not contiguous.
// CHECK-LABEL: fn @gap
// CHECK-NOT: calldatacopy
// CHECK: calldataload 4
// CHECK: calldataload 68

// One word is not a run.
// CHECK-LABEL: fn @single
// CHECK-NOT: calldatacopy
// CHECK: calldataload 4

contract Test {
    function run() external pure {
        assembly {
            let p := mload(0x40)
            mstore(p, calldataload(4))
            mstore(add(p, 32), calldataload(36))
            mstore(add(p, 64), calldataload(68))
            return(p, 96)
        }
    }

    function between() external pure {
        assembly {
            let p := mload(0x40)
            mstore(p, calldataload(4))
            mstore(0, 1)
            mstore(add(p, 32), calldataload(36))
            return(p, 64)
        }
    }

    function gap() external pure {
        assembly {
            let p := mload(0x40)
            mstore(p, calldataload(4))
            mstore(add(p, 32), calldataload(68))
            return(p, 64)
        }
    }

    function single() external pure {
        assembly {
            let p := mload(0x40)
            mstore(p, calldataload(4))
            return(p, 32)
        }
    }
}
