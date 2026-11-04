contract Tuples {
    uint256[] storageArray;
    uint256[2][2] storageNested;

    function pair() internal pure returns (address[] memory, uint256[] memory) {}

    function triple() internal pure returns (uint256, address[] memory, address[] memory) {}

    function take(uint8[3] memory) internal pure {}

    function statements(bool c) internal {
        // Only the first variable of a tuple declaration has an array type without a state
        // mutability of its address.
        (address[] memory a, address[] memory b) = (new address[](1), new address[](2));
        (, address[] memory d, address[] memory e) = triple();
        address[] memory single = new address[](1);
        // An inline array has the common mobile type of its elements, not the type of its
        // destination.
        storageArray = [1, 2];
        storageNested = [[10, 11], [12, 13]];
        take([3, 3, 3]);
        uint16[2] memory wide = [1, 300];
        require(c, (CustomError()));
    }
}

error CustomError();
