//@ codegen-matrix: standard
//@ run-call: MutableSharedRootsNonzero::run 256, 256, 0; constructor=[0], gas=1000000 => 0xdeadbeef, 171, 19, 9
//@ run-call: MutableSharedRootsNonzero::run 257, 257, 17; constructor=[0], gas=1000000 => 0xdeadbeef, 171, 19, 9
//@ run-call: MutableSharedRootsNonzero::run 576, 576, 9; constructor=[0], gas=1000000 => 0xdeadbeef, 171, 19, 9
//@ run-call: MutableSharedRootsDefined::runDefined 4096, 4128, 100; constructor=[0], gas=1000000 => 4660, 171, 19, 9
//@ run-call: MutableSharedRootsNonzero::run 576, 576, 17; constructor=[1], gas=1000000 => 0xdeadbeef, 152, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 9
//@ run-call: MutableSharedRootsDefined::runDefined 4096, 4128, 0; constructor=[1], gas=1000000 => 4660, 152, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 9

// Capture nonzero storage before an unknown aliasing write; do not reissue SLOADs.
// Each root feeds two reductions after the raw memory operation.
// No memory-safe annotation changes the source memory contract.
contract MutableSharedRootsNonzero {
    constructor(uint256 wrap) {
        assembly {
            for { let i := 0 } lt(i, 18) { i := add(i, 1) } {
                sstore(i, add(i, 1))
            }
            if wrap { sstore(17, not(0)) }
        }
    }

    function run(uint256 destination, uint256 source, uint256 changedSlot)
        external returns (uint256 observed, uint256 sum, uint256 parity, uint256 stored)
    {
        assembly {
            let a00 := sload(0)
            let a01 := sload(1)
            let a02 := sload(2)
            let a03 := sload(3)
            let a04 := sload(4)
            let a05 := sload(5)
            let a06 := sload(6)
            let a07 := sload(7)
            let a08 := sload(8)
            let a09 := sload(9)
            let a10 := sload(10)
            let a11 := sload(11)
            let a12 := sload(12)
            let a13 := sload(13)
            let a14 := sload(14)
            let a15 := sload(15)
            let a16 := sload(16)
            let a17 := sload(17)
            sstore(changedSlot, 9)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            sum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
            parity := xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
            stored := sload(changedSlot)
        }
    }
}

// A separate owner isolates the explicitly initialized disjoint-read control.
contract MutableSharedRootsDefined {
    constructor(uint256 wrap) {
        assembly {
            for { let i := 0 } lt(i, 18) { i := add(i, 1) } {
                sstore(i, add(i, 1))
            }
            if wrap { sstore(17, not(0)) }
        }
    }

    function runDefined(uint256 destination, uint256 source, uint256 changedSlot)
        external returns (uint256 observed, uint256 sum, uint256 parity, uint256 stored)
    {
        assembly {
            mstore(source, 0x1234)
            let a00 := sload(0)
            let a01 := sload(1)
            let a02 := sload(2)
            let a03 := sload(3)
            let a04 := sload(4)
            let a05 := sload(5)
            let a06 := sload(6)
            let a07 := sload(7)
            let a08 := sload(8)
            let a09 := sload(9)
            let a10 := sload(10)
            let a11 := sload(11)
            let a12 := sload(12)
            let a13 := sload(13)
            let a14 := sload(14)
            let a15 := sload(15)
            let a16 := sload(16)
            let a17 := sload(17)
            sstore(changedSlot, 9)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            sum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
            parity := xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
            stored := sload(changedSlot)
        }
    }
}
