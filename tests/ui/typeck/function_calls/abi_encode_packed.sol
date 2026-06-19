//@ compile-flags: -Ztypeck

contract C {
    struct S {
        uint256 value;
    }

    string[] storageStrings;
    bytes[] storageBytes;

    function test(
        S[] memory structs,
        uint256[][] memory nestedArray,
        string[] memory memoryStrings,
        bytes[] memory memoryBytes,
        bytes32[] memory fixedBytesArray
    ) public view {
        abi.encodePacked(uint8(1), fixedBytesArray);

        abi.encodePacked(1); //~ ERROR: cannot perform packed encoding for a literal
        abi.encodePacked(structs); //~ ERROR: type not supported in packed mode
        abi.encodePacked(nestedArray); //~ ERROR: type not supported in packed mode
        abi.encodePacked(memoryStrings); //~ ERROR: type not supported in packed mode
        abi.encodePacked(memoryBytes); //~ ERROR: type not supported in packed mode
        abi.encodePacked(storageStrings); //~ ERROR: type not supported in packed mode
        abi.encodePacked(storageBytes); //~ ERROR: type not supported in packed mode
    }
}
