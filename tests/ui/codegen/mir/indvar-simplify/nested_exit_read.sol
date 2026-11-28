//@ codegen-matrix: standard
//@ run-call: find 5, 16 => 4
//@ run-call: find 4, 16 => 14
//@ run-call: find 4, 14 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: find 0, 1 => 0
//@ run-call: find 7, 16 => 0
pragma solidity ^0.8.0;

// The search leaves the table loop through a scan that compares against the table
// index. Once a pointer replaces the index in the table loop, the scan reads the
// index rebuilt in its preheader.
contract NestedExitRead {
    function find(uint256 key, uint256 n) external pure returns (uint256) {
        uint256[16] memory table;
        for (uint256 i = 0; i < 16; i++) {
            table[i] = i * 3 % 7;
        }
        for (uint256 l = 0; l < 16; l++) {
            if (table[l] == key) {
                for (uint256 x = 0; x < n; x++) {
                    if (x * 5 % 16 == l) return x;
                }
                return type(uint256).max;
            }
        }
        return 0;
    }
}
