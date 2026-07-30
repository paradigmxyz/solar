//@run-call: verifyStructCopy 0xaabbccdd => 4660, 1, 0xaabbccdd
//@run-call: verifyShortFixedCopy() => 1, 2, 0
//@run-call: verifyFixedToDynamicCopy() => 2, 3, 5
//@run-call: verifyTupleShortFixedCopy() => 1, 2, 0, 77
//@run-call: verifyTupleFixedToDynamicCopy() => 2, 3, 5, 88

// Copying a memory struct with dynamic fields (bytes/string/dynamic arrays)
// into storage — via assignment or a struct-element array push — writes each
// field's storage length and payload, not the memory pointer word. Verified
// behaviorally against solc, including struct-element push and pop.

contract StorageStructDeepCopy {
    struct Sel {
        address addr;
        bytes4[] selectors;
    }

    Sel single;
    Sel[] list;
    uint256[3] fixedValues;
    uint256[] dynamicValues;

    // SDC-LABEL: fn @assign
    // The dynamic array field's length and elements are written to storage.
    // SDC: sstore {{v[0-9]+}}, {{v[0-9]+}}
    // SDC: sstore 1,
    function assign(address a, bytes4 x) public {
        bytes4[] memory ss = new bytes4[](1);
        ss[0] = x;
        single = Sel(a, ss);
    }

    // SDC-LABEL: fn @push
    // A struct element occupies multiple slots; its dynamic field deep-copies.
    // SDC: keccak256
    // SDC: sstore
    function push(address a, bytes4 x) public {
        bytes4[] memory ss = new bytes4[](1);
        ss[0] = x;
        list.push(Sel(a, ss));
    }

    // SDC-LABEL: fn @pop
    // SDC: sstore
    function pop() public {
        list.pop();
    }

    function verifyStructCopy(bytes4 replacement) external returns (uint256, uint256, bytes4) {
        bytes4[] memory first = new bytes4[](2);
        first[0] = 0x11223344;
        first[1] = 0x55667788;
        single = Sel(address(0x1234), first);

        bytes4[] memory second = new bytes4[](1);
        second[0] = replacement;
        single.selectors = second;
        return (uint160(single.addr), single.selectors.length, single.selectors[0]);
    }

    function verifyShortFixedCopy() external returns (uint256, uint256, uint256) {
        fixedValues = [uint256(9), 9, 9];
        uint256[2] memory source = [uint256(1), 2];
        fixedValues = source;
        return (fixedValues[0], fixedValues[1], fixedValues[2]);
    }

    function verifyFixedToDynamicCopy() external returns (uint256, uint256, uint256) {
        dynamicValues.push(9);
        dynamicValues.push(9);
        dynamicValues.push(9);
        uint256[2] memory source = [uint256(3), 5];
        dynamicValues = source;
        return (dynamicValues.length, dynamicValues[0], dynamicValues[1]);
    }

    function verifyTupleShortFixedCopy()
        external
        returns (uint256, uint256, uint256, uint256)
    {
        fixedValues = [uint256(9), 9, 9];
        uint256[2] memory source = [uint256(1), 2];
        uint256 marker;
        (fixedValues, marker) = (source, 77);
        return (fixedValues[0], fixedValues[1], fixedValues[2], marker);
    }

    function verifyTupleFixedToDynamicCopy()
        external
        returns (uint256, uint256, uint256, uint256)
    {
        dynamicValues.push(9);
        dynamicValues.push(9);
        dynamicValues.push(9);
        uint256[2] memory source = [uint256(3), 5];
        uint256 marker;
        (dynamicValues, marker) = (source, 88);
        return (dynamicValues.length, dynamicValues[0], dynamicValues[1], marker);
    }
}
