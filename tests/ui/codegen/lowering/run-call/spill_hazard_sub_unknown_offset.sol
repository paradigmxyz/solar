//@ codegen-matrix: standard
//@ run-call: f 0x80, 0x500, 1 => 121645100408832001

// `sub(p, sub(p, x))` equals `x` even when `p` is the free-memory pointer.
// Subtracting an unknown amount from a heap pointer can reach the spill area,
// so the copy must be treated as a spill hazard.

contract C {
    function f(uint x, uint n, uint s) external pure returns (uint) {
        uint a1 = s + 1; uint a2 = s + 2; uint a3 = s + 3; uint a4 = s + 4;
        uint a5 = s + 5; uint a6 = s + 6; uint a7 = s + 7; uint a8 = s + 8;
        uint a9 = s + 9; uint a10 = s + 10; uint a11 = s + 11; uint a12 = s + 12;
        uint a13 = s + 13; uint a14 = s + 14; uint a15 = s + 15; uint a16 = s + 16;
        uint a17 = s + 17; uint a18 = s + 18;
        assembly { let p := mload(0x40) calldatacopy(sub(p, sub(p, x)), calldatasize(), n) }
        return a1 ^ a2 ^ a3 ^ a4 ^ a5 ^ a6 ^ a7 ^ a8 ^ a9 ^ a10 ^ a11 ^ a12 ^ a13 ^ a14 ^ a15 ^ a16 ^ a17 ^ a18
            ^ (a1 * a2 * a3 * a4 * a5 * a6 * a7 * a8 * a9 * a10 * a11 * a12 * a13 * a14 * a15 * a16 * a17 * a18);
    }
}
