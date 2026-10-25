//@ revisions: cancun paris
//@[cancun] compile-flags: -Osize --evm-version cancun -Zdump=mir
//@[cancun] filecheck: --check-prefixes=CHECK,CANCUN
//@[paris] compile-flags: -Osize --evm-version paris -Zdump=mir
//@[paris] filecheck: --check-prefixes=CHECK,PARIS

// Loading the three words before storing any of them is a move, whatever the pointers alias.
// CHECK-LABEL: fn @loadsFirst
// CANCUN: mcopy {{v[0-9]+}}, {{v[0-9]+}}, 96
// CANCUN-NOT: mload {{v[0-9]+}}
// PARIS-NOT: mcopy

// Stores that step down onto the words they load are a move, as the destination starts below
// the source.
// CHECK-LABEL: fn @shiftDown
// CANCUN: mcopy {{v[0-9]+}}, {{v[0-9]+}}, 96
// PARIS-NOT: mcopy

// The destination may overlap the source above it, where one store feeds a later load, so
// word-by-word copying differs from `mcopy`.
// CHECK-LABEL: fn @shiftBy
// CHECK-NOT: mcopy
// CHECK: mload {{v[0-9]+}}
// CHECK: mstore {{v[0-9]+}}, {{v[0-9]+}}

// A store between the loads may write the source.
// CHECK-LABEL: fn @between
// CHECK-NOT: mcopy

// A loop that writes the source runs between the loads and the stores.
// CHECK-LABEL: fn @loop
// CHECK-NOT: mcopy

contract Test {
    function loadsFirst(bytes memory data) external pure {
        assembly {
            let s := add(data, 32)
            let d := mload(0x40)
            let a := mload(s)
            let b := mload(add(s, 32))
            let c := mload(add(s, 64))
            mstore(d, a)
            mstore(add(d, 32), b)
            mstore(add(d, 64), c)
            return(d, 96)
        }
    }

    function shiftDown(bytes memory data) external pure {
        assembly {
            let s := add(data, 32)
            mstore(s, mload(add(s, 32)))
            mstore(add(s, 32), mload(add(s, 64)))
            mstore(add(s, 64), mload(add(s, 96)))
            return(s, 96)
        }
    }

    function shiftBy(bytes memory data, uint256 off) external pure {
        assembly {
            let s := add(data, 32)
            let d := add(s, off)
            mstore(d, mload(s))
            mstore(add(d, 32), mload(add(s, 32)))
            mstore(add(d, 64), mload(add(s, 64)))
            return(s, 160)
        }
    }

    function between(bytes memory data) external pure {
        assembly {
            let s := add(data, 32)
            let d := mload(0x40)
            let a := mload(s)
            mstore(add(s, 32), 9)
            let b := mload(add(s, 32))
            mstore(d, a)
            mstore(add(d, 32), b)
            return(d, 64)
        }
    }

    function loop(bytes memory data, uint256 n) external pure {
        assembly {
            let s := add(data, 32)
            let d := mload(0x40)
            let a := mload(s)
            let b := mload(add(s, 32))
            for { let i := 0 } lt(i, n) { i := add(i, 1) } { mstore(s, i) }
            mstore(d, a)
            mstore(add(d, 32), b)
            return(d, 64)
        }
    }
}
