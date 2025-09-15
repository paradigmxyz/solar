//@ compile-flags: -Zprint-max-storage-sizes

// Okay because 2^256 - 1 slots, which are the maximum permissible slots, are used
contract AlmostIllegal { //~ NOTE: :AlmostIllegal requires a maximum of 115792089237316195423570985008687907853269984665640564039457584007913129639935 storage slots
    uint256[115792089237316195423570985008687907853269984665640564039457584007913129639935]
        public x;
}

// Not Okay because 2^256 slots are used
contract Illegal { //~ ERROR: contract requires too much storage
    uint256[115792089237316195423570985008687907853269984665640564039457584007913129639935]
        public x;
    uint256 y;
}
