// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Reading ranges of another account's code.
/// @dev Compiler-owned module, imported as `solar:core/v1/Code.sol`.
/// `target.code` copies the whole code; these copy a range of it, checked
/// against the code's size rather than zero-padded past it, which is what
/// reading a data contract needs. The body is one memory-safe assembly copy,
/// and the compiler lowers the call to the copy instruction with the same
/// operands. A range outside the code or the buffer raises `Panic(0x32)`.
library Code {
    /// @dev Copies `count` bytes of `target`'s code from `start` into `dst`
    /// at `dstOffset`.
    function copyInto(bytes memory dst, uint256 dstOffset, address target, uint256 start, uint256 count)
        internal
        view
    {
        if (count > dst.length || dstOffset > dst.length - count) count = _outOfBounds();
        uint256 size = target.code.length;
        if (count > size || start > size - count) count = _outOfBounds();
        assembly ("memory-safe") {
            extcodecopy(target, add(add(dst, 0x20), dstOffset), start, count)
        }
    }

    /// @dev The `count` bytes of `target`'s code from `start`, as a new buffer.
    function read(address target, uint256 start, uint256 count)
        internal
        view
        returns (bytes memory out)
    {
        out = new bytes(count);
        copyInto(out, 0, target, start, count);
    }

    /// @dev Raises the `Panic(0x32)` an out-of-range index raises, which is the
    /// portable spelling of a failed range check.
    function _outOfBounds() private pure returns (uint256) {
        return new uint256[](0)[0];
    }
}
