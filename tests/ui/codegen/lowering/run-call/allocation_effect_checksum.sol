//@ codegen-matrix: standard
//@ run-call: checksum [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0], 0 => (0), 0, 32
//@ run-call: checksum [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16], 9 => (9), 136, 32
// Observe the allocated extent so the source allocation cannot be deferred.
// The checksum inputs must survive allocation until their pure consumers run.
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AllocationEffectChecksum {
    struct Cell { uint256 marker; }

    function checksum(uint256[17] calldata values, uint256 marker)
        external pure returns (Cell memory cell, uint256 sum, uint256 allocatedBytes)
    {
        uint256 a0;
        uint256 a1;
        uint256 a2;
        uint256 a3;
        uint256 a4;
        uint256 a5;
        uint256 a6;
        uint256 a7;
        uint256 a8;
        uint256 a9;
        uint256 a10;
        uint256 a11;
        uint256 a12;
        uint256 a13;
        uint256 a14;
        uint256 a15;
        uint256 a16;
        assembly {
            a0 := calldataload(add(values, 0))
            a1 := calldataload(add(values, 32))
            a2 := calldataload(add(values, 64))
            a3 := calldataload(add(values, 96))
            a4 := calldataload(add(values, 128))
            a5 := calldataload(add(values, 160))
            a6 := calldataload(add(values, 192))
            a7 := calldataload(add(values, 224))
            a8 := calldataload(add(values, 256))
            a9 := calldataload(add(values, 288))
            a10 := calldataload(add(values, 320))
            a11 := calldataload(add(values, 352))
            a12 := calldataload(add(values, 384))
            a13 := calldataload(add(values, 416))
            a14 := calldataload(add(values, 448))
            a15 := calldataload(add(values, 480))
            a16 := calldataload(add(values, 512))
        }
        cell = Cell(marker);
        assembly {
            sum := add(a0, a1)
            sum := add(sum, a2)
            sum := add(sum, a3)
            sum := add(sum, a4)
            sum := add(sum, a5)
            sum := add(sum, a6)
            sum := add(sum, a7)
            sum := add(sum, a8)
            sum := add(sum, a9)
            sum := add(sum, a10)
            sum := add(sum, a11)
            sum := add(sum, a12)
            sum := add(sum, a13)
            sum := add(sum, a14)
            sum := add(sum, a15)
            sum := add(sum, a16)
            allocatedBytes := sub(mload(0x40), cell)
        }
    }
}
