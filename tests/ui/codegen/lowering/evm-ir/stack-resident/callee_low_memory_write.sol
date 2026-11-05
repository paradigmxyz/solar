//@ codegen-matrix: standard
//@ run-call: f 7, 0 => 114047703
//@ run-call: f 7, 32 => 114047735
//@ run-call: f 7, 1024 => 114048727
//@ run-call: f 7, 4096 => 114051799
//@ run-call: f 1, 2048 => 16294577
// `f` keeps 18 words live across the call to `g`, so some of them spill to low memory
// above 0x80. `g` copies `n` bytes of code to address 0 and restores the free-memory
// pointer and zero slot. The spill-hazard analysis only pins values live across a function's
// own low-memory writes, so `f` spills across the call and `g` overwrites those slots once
// `n` reaches them. The recursive guard keeps `g` from being inlined into `f`.

contract C {
    function g(uint256 n) internal view returns (uint256 r) {
        if (n == 12345) return g(n + 1);
        assembly {
            let p := mload(0x40)
            codecopy(0, 0, n)
            mstore(0x40, p)
            mstore(0x60, 0)
            r := n
        }
    }

    function f(uint256 a, uint256 n) external view returns (uint256) {
        unchecked {
            uint256 b1 = a * 3; uint256 b2 = a * 5; uint256 b3 = a * 7; uint256 b4 = a * 11;
            uint256 b5 = a * 13; uint256 b6 = a * 17; uint256 b7 = a * 19; uint256 b8 = a * 23;
            uint256 b9 = a * 29; uint256 b10 = a * 31; uint256 b11 = a * 37; uint256 b12 = a * 41;
            uint256 b13 = a * 43; uint256 b14 = a * 47; uint256 b15 = a * 53; uint256 b16 = a * 59;
            uint256 b17 = a * 61; uint256 b18 = a * 67;
            uint256 r = g(n);
            return r + b1 + (b2 << 1) + (b3 << 2) + (b4 << 3) + (b5 << 4) + (b6 << 5) + (b7 << 6)
                + (b8 << 7) + (b9 << 8) + (b10 << 9) + (b11 << 10) + (b12 << 11) + (b13 << 12)
                + (b14 << 13) + (b15 << 14) + (b16 << 15) + (b17 << 16) + (b18 << 17);
        }
    }
}
