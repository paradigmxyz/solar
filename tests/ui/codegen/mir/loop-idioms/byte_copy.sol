//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: slice 0x4142434445, 1, 4 => 0x424344
//@ run-call: slice 0x4142434445, 0, 99 => 0x4142434445
//@ run-call: slice 0x4142434445, 3, 2 => 0x
//@ run-call: slice 0x, 0, 0 => 0x
//@ run-call: inPlace 0x41424344 => 0x42434444

// Source and destination in one object keep the loop, because `mcopy` moves as
// if through a buffer while the loop copies upwards, so overlapping ranges give
// different results. The copy returning the value is unrelated.
// CHECK-LABEL: fn @inPlace
// CHECK: mstore8

// A loop whose whole body moves one byte from a caller's object into a buffer
// this function allocated becomes one copy in the preheader. The clamp gives
// the compiler `end <= s.length`, and the loop bound is `end - start`, which
// together put `start + k` inside the source.
// CHECK-LABEL: fn @_slice
// CHECK: mcopy
// CHECK-NOT: mstore8

contract Test {
    function slice(bytes memory s, uint256 start, uint256 end) public pure returns (bytes memory) {
        return _slice(s, start, end);
    }

    function _slice(bytes memory s, uint256 start, uint256 end)
        private
        pure
        returns (bytes memory out)
    {
        if (end > s.length) end = s.length;
        if (start > s.length) start = s.length;
        if (start >= end) return "";
        out = new bytes(end - start);
        for (uint256 k; k < out.length; ++k) out[k] = s[start + k];
    }

    // Both ranges are the same object, so the ordering above does not apply.
    function inPlace(bytes memory s) public pure returns (bytes memory) {
        for (uint256 k; k + 1 < s.length; ++k) s[k] = s[k + 1];
        return s;
    }
}
