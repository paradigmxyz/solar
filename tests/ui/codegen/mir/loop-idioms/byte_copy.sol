//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: slice 0x4142434445, 1, 4 => 0x424344
//@ run-call: slice 0x4142434445, 0, 99 => 0x4142434445
//@ run-call: slice 0x4142434445, 3, 2 => 0x
//@ run-call: slice 0x, 0, 0 => 0x
//@ run-call: inPlace 0x41424344 => 0x42434444
//@ run-call: rawOverlap => 0x41414141
//@ run-call: objectOverlap => 0x41414141
//@ run-call: allocationOverlap => 8
//@ run-call: oversizedCopy => 65
//@ run-call: zeroTripHeader 0 => 0
//@ run-call: headerExpansion 0 => false
//@ run-call: headerExpansion 1 => true

// Source and destination in one object keep the loop, because `mcopy` moves as
// if through a buffer while the loop copies upwards, so overlapping ranges give
// different results. The copy returning the value is unrelated.
// CHECK-LABEL: fn @inPlace
// CHECK: mstore8

// The output's allocation cannot change the input's length: its fill and its
// length word stay inside it, and its pointer bump is a reserved word. So the
// length read before it is the one the loop compares against, the source
// bounds check folds into the loop condition, and the byte loop is one `mcopy`.
// CHECK-LABEL: fn @_slice
// CHECK: [[INPUT:v[0-9]+]] = ptrtoint memptr arg0 to i256
// CHECK: mload [[INPUT]]
// CHECK-NOT: mload [[INPUT]]
// CHECK-NOT: mstore8
// CHECK: mcopy

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

    function rawOverlap() external pure returns (bytes memory out) {
        uint256 source;
        assembly {
            source := add(mload(64), 31)
            mstore8(source, 0x41)
        }
        out = copyRaw(source, 4);
    }

    function objectOverlap() external pure returns (bytes memory out) {
        bytes memory source;
        assembly {
            source := sub(mload(64), 1)
            mstore8(add(source, 32), 0x41)
        }
        out = copyObject(source, 4);
    }

    function allocationOverlap() external pure returns (uint256) {
        bytes memory source;
        assembly {
            source := mload(64)
            mstore(source, 1)
        }
        return allocateOver(source);
    }

    function allocateOver(bytes memory source) private pure returns (uint256) {
        uint256 beforeLength = source.length;
        bytes memory out = new bytes(7);
        return beforeLength + source.length + out.length - 7;
    }

    function oversizedCopy() external pure returns (uint256) {
        bytes memory source = new bytes(32);
        bytes memory dest = new bytes(65);
        source[0] = 0x41;
        assembly {
            for { let i := 0 } lt(i, 65) { i := add(i, 1) } {
                mstore8(add(add(dest, 32), i), byte(0, mload(add(add(source, 32), i))))
            }
        }
        return uint8(dest[64]);
    }

    function zeroTripHeader(uint256 count) external pure returns (uint256) {
        bytes memory source;
        assembly { source := not(0) }
        return readHeaders(source, count);
    }

    function readHeaders(bytes memory source, uint256 count) private pure returns (uint256 result) {
        for (uint256 i; i < count; ++i) result = source.length;
    }

    function headerExpansion(uint256 count) external pure returns (bool result) {
        bytes memory source;
        assembly { source := 0x10000 }
        for (uint256 i; i < count; ++i) {
            uint256 beforeSize = readSize();
            uint256 length = source.length;
            uint256 afterSize = readSize();
            result = afterSize > beforeSize && length == 0;
        }
    }

    function readSize() private pure returns (uint256 size) {
        assembly { size := msize() }
    }

    function copyObject(bytes memory source, uint256 length) private pure returns (bytes memory out) {
        assembly {
            out := mload(64)
            let dest := add(out, 32)
            mstore(64, add(dest, length))
            for { let i := 0 } lt(i, length) { i := add(i, 1) } {
                mstore8(add(dest, i), byte(0, mload(add(add(source, 32), i))))
            }
            mstore(out, length)
        }
    }

    function copyRaw(uint256 source, uint256 length) private pure returns (bytes memory out) {
        assembly {
            out := mload(64)
            let dest := add(out, 32)
            mstore(64, add(dest, length))
            for { let i := 0 } lt(i, length) { i := add(i, 1) } {
                mstore8(add(dest, i), byte(0, mload(add(source, i))))
            }
            mstore(out, length)
        }
    }

    // Both ranges are the same object, so the ordering above does not apply.
    function inPlace(bytes memory s) public pure returns (bytes memory) {
        for (uint256 k; k + 1 < s.length; ++k) s[k] = s[k + 1];
        return s;
    }
}
