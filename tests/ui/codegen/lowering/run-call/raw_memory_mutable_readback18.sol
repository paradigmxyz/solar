//@ codegen-matrix: standard
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 192, 192; gas=1000000 => 0xdeadbeef, 4522, 9
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 193, 193; gas=1000000 => 0xdeadbeef, 4522, 9
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 223, 223; gas=1000000 => 0xdeadbeef, 4522, 9
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 224, 224; gas=1000000 => 0xdeadbeef, 4522, 9
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096; gas=1000000 => 0xdeadbeef, 4522, 9
//@ run-call: run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4128; gas=1000000 => 0x0, 4522, 9

// Compiler-private storage must preserve source memory and captured SSA values.
// The first SLOAD captures zero before SSTORE changes the slot to nine.
// Reissuing that read would change checksum 4522 to 4531.
// Address 192 is a passing control; 193/223/224 expose partial or whole overlap.
// The 4096 control retains the same program and live values with another address.
// No memory-safe annotation or backend layout assumption grants private memory.

contract LogicalMemoryMixedRoots {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external returns (uint256 observed, uint256 checksum, uint256 stored)
    {
        assembly {
            let a00 := sload(0)
            let a01 := xor(calldataload(add(values, 32)), 256)
            let a02 := xor(calldataload(add(values, 64)), 256)
            let a03 := xor(calldataload(add(values, 96)), 256)
            let a04 := xor(calldataload(add(values, 128)), 256)
            let a05 := xor(calldataload(add(values, 160)), 256)
            let a06 := xor(calldataload(add(values, 192)), 256)
            let a07 := xor(calldataload(add(values, 224)), 256)
            let a08 := xor(calldataload(add(values, 256)), 256)
            let a09 := xor(calldataload(add(values, 288)), 256)
            let a10 := xor(calldataload(add(values, 320)), 256)
            let a11 := xor(calldataload(add(values, 352)), 256)
            let a12 := xor(calldataload(add(values, 384)), 256)
            let a13 := xor(calldataload(add(values, 416)), 256)
            let a14 := xor(calldataload(add(values, 448)), 256)
            let a15 := xor(calldataload(add(values, 480)), 256)
            let a16 := xor(calldataload(add(values, 512)), 256)
            let a17 := xor(calldataload(add(values, 544)), 256)
            sstore(0, 9)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
            stored := sload(0)
        }
    }
}
