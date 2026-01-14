//@compile-flags: -Ztypeck

// Tests for array slice implicit conversions.
// See: https://docs.soliditylang.org/en/latest/types.html#array-slices
contract C {
    // Array slices are implicitly convertible to arrays of their underlying type.
    function ok(uint256[] calldata data, uint256 start, uint256 end) external pure {
        // Take a slice and implicitly convert it to the underlying type.
        uint256[] calldata s = data[start:end];
    }

    // Slices cannot be implicitly converted to a different element type.
    function wrongElementType(uint256[] calldata data, uint256 start, uint256 end) external pure {
        uint128[] calldata s = data[start:end]; //~ ERROR: mismatched types
    }

    // Slices can be implicitly converted to memory arrays of the same element type.
    function toMemory(uint256[] calldata data, uint256 start, uint256 end) external pure {
        uint256[] memory s = data[start:end];
    }

    // Slices can only be created from calldata arrays.
    function notCalldata(uint256[] memory data, uint256 start, uint256 end) external pure {
        uint256[] memory s = data[start:end]; //~ ERROR: can only slice dynamic calldata arrays
    }

    // Slices cannot be converted to a single element.
    function wrongType(uint256[] calldata data, uint256 start, uint256 end) external pure {
        uint256 a = data[start:end]; //~ ERROR: mismatched types
    }

    // Slices cannot be assigned to storage pointers.
    uint256[] s;
    function toStoragePointer(uint256[] calldata data, uint256 start, uint256 end) external {
    //~^ WARN: function state mutability can be restricted to view
        uint256[] storage t = s;
        t = data[start:end]; //~ ERROR: mismatched types
    }
}
