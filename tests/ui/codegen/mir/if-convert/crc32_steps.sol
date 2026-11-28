//@ codegen-matrix: standard
//@ run-call: crc32 0x => 0
//@ run-call: crc32 0x313233343536373839 => 0xcbf43926
//@ run-call: crc32 0xff => 0xff000000
pragma solidity ^0.8.0;

// The bitwise CRC-32 of gzip: each step xors the polynomial in only when the dropped bit was
// set, which needs no branch.
contract Crc32Steps {
    function crc32(bytes memory data) external pure returns (uint256 crc) {
        assembly {
            crc := 0xffffffff
            let ptr := add(data, 0x20)
            for { let end := add(ptr, mload(data)) } lt(ptr, end) { ptr := add(ptr, 1) } {
                crc := xor(crc, shr(248, mload(ptr)))
                for { let j := 0 } lt(j, 8) { j := add(j, 1) } {
                    let lsb := and(crc, 1)
                    crc := shr(1, crc)
                    if lsb { crc := xor(crc, 0xedb88320) }
                }
            }
            crc := xor(crc, 0xffffffff)
        }
    }
}
