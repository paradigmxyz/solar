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

// The raw allocation frontier has no disjointness proof for the input header.
// Keep the source bounds check and byte loop when that length cannot be reused.
// CHECK-LABEL: fn @_slice
// CHECK: [[INPUT:v[0-9]+]] = ptrtoint memptr arg0 to i256
// CHECK: mload [[INPUT]]
// CHECK: mload [[INPUT]]
// CHECK: mstore8
// CHECK-NOT: mcopy

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
