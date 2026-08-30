contract TernaryDataLocations {
    uint256[] private words;

    function incompatibleReferences(bool condition) external {
        uint256[] memory a = new uint256[](1);
        bytes memory b = hex"aa";
        words = condition ? a : b; //~ ERROR: incompatible conditional types
    }

    function memoryToStoragePointer(bool condition) external {
        uint256[] memory a = new uint256[](1);
        uint256[] storage pointer = condition ? words : a; //~ ERROR: mismatched types
        pointer;
    }

    function incompatibleScalars(bool condition) external {
        uint256 value = condition ? 1 : true; //~ ERROR: incompatible conditional types
        value;
    }

    function storageCalldata(bool condition, uint256[] calldata a) external {
        words = condition ? words : a; //~ ERROR: incompatible conditional types
    }
}
