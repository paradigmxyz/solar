// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @dev A range of an account's code that was inside the code when
/// `Code.slice` made it: the account, where the range starts, and how many
/// bytes it holds, packed into one word. It is a value, not memory, and only
/// `Code` makes or unpacks one, which this compiler enforces. Code can change
/// after a view is made, so every read checks the range against the code
/// again rather than trusting the view.
type CodeView is uint256;

/// @notice Reading ranges of another account's code.
/// @dev Compiler-owned module, imported as `solar:core/v1/Code.sol`.
/// `target.code` copies the whole code; these copy a range of it, checked
/// against the code's size rather than zero-padded past it, which is what
/// reading a data contract needs. The body is one memory-safe assembly copy,
/// and the compiler lowers the call to the copy instruction with the same
/// operands; `read` it lowers to that copy into a buffer it does not zero
/// first. A range outside the code or the buffer raises `Panic(0x32)`.
library Code {
    /// @dev The bits a view gives each of its start and its length, far more
    /// than any code size needs.
    uint256 private constant RANGE_BITS = 48;

    /// @dev The mask of one range field of a view.
    uint256 private constant RANGE_MASK = (1 << RANGE_BITS) - 1;

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

    /// @dev A view of the `count` bytes of `target`'s code from `start`, which
    /// must lie inside the code.
    function slice(address target, uint256 start, uint256 count)
        internal
        view
        returns (CodeView)
    {
        uint256 size = target.code.length;
        if (count > size || start > size - count) count = _outOfBounds();
        return _pack(target, start, count);
    }

    /// @dev A view of the `count` bytes of `section` from `start`, which must
    /// lie inside it.
    function slice(CodeView section, uint256 start, uint256 count)
        internal
        pure
        returns (CodeView)
    {
        uint256 total = length(section);
        if (count > total || start > total - count) count = _outOfBounds();
        return _pack(account(section), offset(section) + start, count);
    }

    /// @dev The account whose code `section` is a range of.
    function account(CodeView section) internal pure returns (address) {
        return address(uint160(CodeView.unwrap(section) >> (2 * RANGE_BITS)));
    }

    /// @dev Where `section` starts in its account's code.
    function offset(CodeView section) internal pure returns (uint256) {
        return (CodeView.unwrap(section) >> RANGE_BITS) & RANGE_MASK;
    }

    /// @dev How many bytes `section` holds.
    function length(CodeView section) internal pure returns (uint256) {
        return CodeView.unwrap(section) & RANGE_MASK;
    }

    /// @dev Copies the bytes of `section` into `dst` at `dstOffset`. The range
    /// is checked against the account's code again, since the code can change
    /// after the view is made.
    function copyInto(bytes memory dst, uint256 dstOffset, CodeView section) internal view {
        copyInto(dst, dstOffset, account(section), offset(section), length(section));
    }

    /// @dev The bytes of `section`, as a new buffer, checked as `copyInto`
    /// checks them.
    function read(CodeView section) internal view returns (bytes memory out) {
        return read(account(section), offset(section), length(section));
    }

    /// @dev Packs a view of a range already checked to lie inside some code,
    /// so both of its fields fit in their bits.
    function _pack(address target, uint256 start, uint256 count) private pure returns (CodeView) {
        return CodeView.wrap(
            (uint256(uint160(target)) << (2 * RANGE_BITS)) | (start << RANGE_BITS) | count
        );
    }

    /// @dev Raises the `Panic(0x32)` an out-of-range index raises, which is the
    /// portable spelling of a failed range check.
    function _outOfBounds() private pure returns (uint256) {
        return new uint256[](0)[0];
    }
}
