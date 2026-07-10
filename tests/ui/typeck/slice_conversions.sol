//@ compile-flags: -Ztypeck

contract C {
    function ok(uint256[] calldata data, uint256 start, uint256 end) external pure {
        uint256[] calldata s = data[start:end];
        s;
    }

    function wrongElementType(uint256[] calldata data, uint256 start, uint256 end) external pure {
        uint128[] calldata s = data[start:end]; //~ ERROR: mismatched types
        s;
    }

    function toMemory(uint256[] calldata data, uint256 start, uint256 end) external pure {
        uint256[] memory s = data[start:end];
        s;
    }

    function notCalldata(uint256[] memory data, uint256 start, uint256 end) external pure {
        uint256[] memory s = data[start:end]; //~ ERROR: can only slice dynamic calldata arrays
        s;
    }

    function wrongType(uint256[] calldata data, uint256 start, uint256 end) external pure {
        uint256 a = data[start:end]; //~ ERROR: mismatched types
        a;
    }

    uint256[] s;

    function toStoragePointer(uint256[] calldata data, uint256 start, uint256 end) external {
        uint256[] storage t = s;
        t = data[start:end]; //~ ERROR: mismatched types
    }
}
