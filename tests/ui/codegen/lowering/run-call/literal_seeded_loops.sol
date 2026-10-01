//@ codegen-matrix: standard
//@ run-call: twoLoops 3, 2 => 6, 5, false
//@ run-call: twoLoops 0, 0 => 0, 0, true
//@ run-call: twoLoops 0, 3 => 0, 12, false
//@ run-call: scan 0x6162000000000000000000000000000000000000000000000000000000000000 => 2
//@ run-call: scan 0x0000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: scan 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 32
//@ run-call: scanTwo 0x6162000000000000000000000000000000000000000000000000000000000000, 0x6162636400000000000000000000000000000000000000000000000000000000 => 204
//@ run-call: nested 0 => 0x
//@ run-call: nested 1 => 0x
//@ run-call: nested 4 => 0x102021303132

// Loop counters seeded from a literal that stays live past the loop header,
// here the zero of a later comparison, may be carried on the stack. Two loops
// sharing that literal and allocating inside their bodies must each keep
// their own counter.
contract LiteralSeededLoops {
    function twoLoops(uint256 n, uint256 m) external pure returns (uint256 a, uint256 b, bool empty) {
        for (uint256 i = 0; i < n; ++i) {
            bytes memory x = new bytes(i);
            a += x.length + 1;
        }
        for (uint256 j = 0; j < m; ++j) {
            uint256[] memory y = new uint256[](j + 1);
            y[j] = j;
            b += y[j] * 2 + y.length;
        }
        empty = a == 0 && b == 0;
    }

    function scan(bytes32 s) external pure returns (uint256 i) {
        while (i < 32 && s[i] != 0) ++i;
    }

    function nullIndex(bytes32 s) internal pure returns (uint256 i) {
        while (i < 32 && s[i] != 0) ++i;
    }

    function scanTwo(bytes32 s, bytes32 t) external pure returns (uint256) {
        return nullIndex(s) * 100 + nullIndex(t);
    }

    function nested(uint256 n) external pure returns (bytes memory out) {
        for (uint256 i = 0; i < n; ++i) {
            for (uint256 j = 0; j < i; ++j) {
                out = bytes.concat(out, bytes1(uint8(i * 16 + j)));
            }
        }
    }
}
