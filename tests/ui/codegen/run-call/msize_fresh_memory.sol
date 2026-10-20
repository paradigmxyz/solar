//@ codegen-matrix: standard
//@ run-call: Arrays::first => [11, 22, 33]
//@ run-call: Arrays::second => [11, 22, 33], 22
//@ run-call: Arrays::bumped 7 => [7, 8, 9]
//@ run-call: Arrays::spilled 1, 2, 3, 4, 5, 6 => 7740114806721905576
//@ run-call: Buffer::sum => 912080
//@ run-call: Buffer::gap => 2

// Assembly takes fresh memory at `msize()`. The value is at or above the free
// memory pointer and above everything the compiler placed there, so a buffer
// there overlaps neither spill slots, static frames nor later allocations.
contract Arrays {
    function make() internal pure returns (uint256[3] memory v) {
        assembly {
            v := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
            mstore(v, 11)
            mstore(add(v, 32), 22)
            mstore(add(v, 64), 33)
            mstore(0x40, add(v, 96))
        }
    }

    function first() external pure returns (uint256[3] memory) {
        return make();
    }

    function second() external pure returns (uint256[3] memory v, uint256 gap) {
        v = make();
        gap = v[2] - v[0];
    }

    // The free memory pointer itself becomes `msize()`.
    function bumped(uint256 a) external pure returns (uint256[3] memory out) {
        assembly { mstore(0x40, msize()) } //~[gas,size] WARN: `msize` is not exact when optimizing
        out = [a, a + 1, a + 2];
    }

    function makeDynamic(uint256 n) internal pure returns (uint256[] memory v) {
        assembly {
            v := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
            mstore(v, n)
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                mstore(add(add(v, 32), mul(i, 32)), add(i, 100))
            }
            mstore(0x40, add(add(v, 32), mul(n, 32)))
        }
    }

    // Enough live values to spill below the heap while the array exists.
    function spilled(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f)
        external
        pure
        returns (uint256 r)
    {
        uint256[] memory v = makeDynamic(8);
        uint256 x1 = a * b;
        uint256 x2 = b * c;
        uint256 x3 = c * d;
        uint256 x4 = d * e;
        uint256 x5 = e * f;
        uint256 x6 = f * a;
        uint256 x7 = a + c;
        uint256 x8 = b + d;
        uint256 x9 = c + e;
        uint256 x10 = d + f;
        uint256 x11 = a ^ f;
        uint256 x12 = b ^ e;
        uint256 x13 = x1 * x2 + x3;
        uint256 x14 = x4 * x5 + x6;
        uint256 x15 = x7 * x8 + x9;
        uint256 x16 = x10 * x11 + x12;
        r = x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10 + x11 + x12 + x13 + x14 + x15 + x16
            + a + b + c + d + e + f;
        r = r ^ (x1 * x16) ^ (x2 * x15) ^ (x3 * x14) ^ (x4 * x13) ^ (x5 * x12) ^ (x6 * x11);
        for (uint256 i; i < 8; i++) r += v[i] << (i * 8);
    }
}

// A constructor buffer at `msize()` survives the immutable staging that follows it.
contract Buffer {
    uint256 immutable a;
    uint256 immutable b;
    uint256 public total;

    constructor() {
        uint256 p;
        assembly {
            p := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
            for { let i := 0 } lt(i, 0x200) { i := add(i, 32) } { mstore(add(p, i), 0xdead) }
            mstore(0x40, add(p, 0x200))
        }
        a = block.number + 7;
        b = block.number + 9;
        uint256 sum;
        assembly {
            for { let i := 0 } lt(i, 0x200) { i := add(i, 32) } { sum := add(sum, mload(add(p, i))) }
        }
        total = sum;
    }

    function sum() external view returns (uint256) {
        return total;
    }

    function gap() external view returns (uint256) {
        return b - a;
    }
}
