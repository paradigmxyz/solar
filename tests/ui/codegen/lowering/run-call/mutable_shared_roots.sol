//@ codegen-matrix: standard
//@ run-call: run 192, 192, 0; gas=1000000 => 0xdeadbeef, 0, 0, 9
//@ run-call: run 193, 193, 0; gas=1000000 => 0xdeadbeef, 0, 0, 9
//@ run-call: run 223, 223, 17; gas=1000000 => 0xdeadbeef, 0, 0, 9
//@ run-call: run 224, 224, 0; gas=1000000 => 0xdeadbeef, 0, 0, 9
//@ run-call: run 4096, 4096, 0; gas=1000000 => 0xdeadbeef, 0, 0, 9
//@ run-call: run 4096, 4128, 0; gas=1000000 => 0x0, 0, 0, 9

//@ run-call: run 256, 256, 0; gas=1000000 => 0xdeadbeef, 0, 0, 9
//@ run-call: run 257, 257, 0; gas=1000000 => 0xdeadbeef, 0, 0, 9
//@ run-call: run 576, 576, 17; gas=1000000 => 0xdeadbeef, 0, 0, 9

// Each call requires fresh deployment: all storage slots initially contain zero.
// Every captured SLOAD has two consumers after the raw memory write/readback.
// The unknown SSTORE slot may alias any capture, so reissuing a read is not valid.
// No memory-safe annotation grants the compiler ownership of arbitrary source memory.
contract MutableSharedRootsReadback {
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
