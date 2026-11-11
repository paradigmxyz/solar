//@ codegen-matrix: standard checked
//@[checked] compile-flags: -Ogas -Zcheck-analyses=true -Zdump=mir-final
//@[checked] filecheck: --check-prefix=CHECKED
//@ run-call: firstUnset -1, -1 => 64
//@ run-call: firstUnset 1, -1 => 0
//@ run-call: accumulate 4 => 6
//@ run-call: accumulate 0 => 0
//@ run-call: fill 4 => 18
//@ run-call: fill 0 => 0
//@ run-call: guarded 5 => 6
//@ run-call-fail: guarded 11 => 0xa2f431300000000000000000000000000000000000000000000000000000000000000000

// `-Zcheck-analyses=true` tests the memory, storage and control facts passes relied on: the
// length read loop-invariant code motion hoists (fact words `0x110...`), the slot storage promotion
// keeps in memory across a loop (`0x111...`), the zero fill allocation lowering drops and the free
// memory pointer it does not load (`0x112...`), and the call `noreturn-calls` found cannot
// return (`0x113...`). Every fact holds, so each call behaves as it does unchecked.
contract MemoryFacts {
    uint256 public total;
    uint256 public step = 3;

    error Bad(uint256 x);

    // CHECKED-LABEL: fn @firstUnset(
    // CHECKED: phi [{{.*}}: {{0x110[0-9a-f]+}}]
    // CHECKED: mstore 0, 0xed572b27
    function firstUnset(int64 a, int128 b) external pure returns (uint256) {
        bytes memory encoded = abi.encode(a, b);
        for (uint256 i = 0; i < encoded.length; ++i) {
            if (encoded[i] != 0xff) return i;
        }
        return encoded.length;
    }

    // CHECKED-LABEL: fn @accumulate(
    // CHECKED: phi [{{.*}}: {{0x111[0-9a-f]+}}]
    // CHECKED: mstore 0, 0xed572b27
    function accumulate(uint256 n) external returns (uint256) {
        for (uint256 i = 0; i < n; i++) {
            total += i;
        }
        return total;
    }

    // The elements start as the zeros the dropped fill would have written.
    // CHECKED-LABEL: fn @fill(
    // CHECKED: {{0x112[0-9a-f]+}}
    // CHECKED: mstore 0, 0xed572b27
    function fill(uint256 n) external view returns (uint256 s) {
        uint256[] memory out = new uint256[](n % 64);
        for (uint256 i = 0; i < out.length; i++) {
            out[i] += step + i;
        }
        for (uint256 i = 0; i < out.length; i++) {
            s += out[i];
        }
    }

    function spin(uint256 x) internal pure {
        if (x == 0) revert Bad(0);
        spin(x - 1);
        revert Bad(x); //~ WARN: unreachable code
    }

    // CHECKED-LABEL: fn @guarded(
    // CHECKED: phi [{{.*}}: {{0x113[0-9a-f]+}}]
    // CHECKED: mstore 0, 0xed572b27
    function guarded(uint256 x) external pure returns (uint256) {
        if (x > 10) {
            spin(x);
        }
        return x + 1;
    }
}
