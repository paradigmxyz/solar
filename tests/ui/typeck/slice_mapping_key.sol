// Calldata slices convert to location-less reference types like any other
// reference, so they can index mappings: `m[data[1:]]`.

contract C {
    mapping(bytes => uint256) bytesKeyed;
    mapping(uint256 => uint256) valueKeyed;

    function ok(bytes calldata params) external view returns (uint256) {
        return bytesKeyed[params[1:]];
    }

    function bad(uint256[] calldata nums) external view returns (uint256) {
        return bytesKeyed[nums[1:]]; //~ ERROR: mismatched types
    }

    function alsoBad(bytes calldata params) external view returns (uint256) {
        return valueKeyed[params[1:]]; //~ ERROR: mismatched types
    }
}
