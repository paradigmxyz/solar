//@run-call: verifyStructCopy 0xaabbccdd => 4660, 1, 0xaabbccdd
//@run-call: verifyShortFixedCopy() => 1, 2, 0
//@run-call: verifyFixedToDynamicCopy() => 2, 3, 5
//@run-call: verifyTupleShortFixedCopy() => 1, 2, 0, 77
//@run-call: verifyTupleFixedToDynamicCopy() => 2, 3, 5, 88
//@run-call: verifyStorageReferenceCopies() => 2, 3, 5, 3, 187, 4660, 287454020
//@run-call: verifyStorageReferenceKey() => 99
//@run-call: verifyStorageReferenceMemoryValues() => 3, 187, 2, 5
//@run-call: verifyStorageReferenceAbiValues() => 1, 1, 1, 197
//@run-call: verifyStorageReferenceReturns() => 3, 204, 2, 3
//@run-call: verifyStorageReferenceLowLevelCall() => 7
//@run-call: verifyStorageReferenceInternalCall() => 3, 187, 2, 5
//@run-call: verifyStorageReferencePush() => 4660, 287454020, 3, 5, 8
//@run-call: verifyTupleStorageReferenceDeclaration() => 3, 187, 2, 5, 91
//@run-call: verifyTupleStorageReferenceAssignment() => 3, 204, 2, 3, 92
//@run-call: verifyTupleStorageAliasDeclaration() => 7, 93
//@run-call: verifyTupleStorageAliasAssignment() => 8, 94
//@run-call: verifyStorageReferenceMemoryAggregates() => 4660, 287454020, 3, 8
//@run-call: verifyParenthesizedTupleStorageAliasAssignment() => 9, 95
//@run-call: verifyReturnedStorageReferenceDeclaration() => 5, 96
//@run-call: verifyReturnedStorageReferenceAssignment() => 5, 96
//@run-call: verifyReturnedStorageAlias() => 10, 96
//@run-call: verifyNamedTernaryStorageAlias() => 5, 11, 97
//@run-call: verifyTernaryTupleStorageAlias() => 5, 12, 98

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
    uint256[] sourceValues;
    bytes sourceBytes;
    bytes copiedBytes;
    Sel sourceSel;
    Sel copiedSel;
    mapping(bytes => uint256) keyed;
    uint256[3] sourceFixed;
    uint256[3][] fixedLists;

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

    function verifyStorageReferenceCopies()
        external
        returns (uint256, uint256, uint256, uint256, uint256, uint256, uint256)
    {
        sourceValues.push(3);
        sourceValues.push(5);
        uint256[] storage values = sourceValues;
        dynamicValues = values;

        sourceBytes = hex"aabbcc";
        bytes storage data = sourceBytes;
        copiedBytes = data;

        bytes4[] memory selectors = new bytes4[](1);
        selectors[0] = bytes4(0x11223344);
        sourceSel = Sel(address(0x1234), selectors);
        Sel storage selection = sourceSel;
        copiedSel = selection;

        return (
            dynamicValues.length,
            dynamicValues[0],
            dynamicValues[1],
            copiedBytes.length,
            uint8(copiedBytes[1]),
            uint160(copiedSel.addr),
            uint32(copiedSel.selectors[0])
        );
    }

    function verifyStorageReferenceKey() external returns (uint256) {
        sourceBytes = hex"aabbcc";
        bytes storage data = sourceBytes;
        keyed[data] = 99;
        return keyed[hex"aabbcc"];
    }

    function initializeReferenceValues() internal {
        sourceBytes = hex"aabbcc";
        sourceValues.push(3);
        sourceValues.push(5);
    }

    function verifyStorageReferenceMemoryValues()
        external
        returns (uint256, uint256, uint256, uint256)
    {
        initializeReferenceValues();
        bytes storage data = sourceBytes;
        uint256[] storage values = sourceValues;
        bytes memory memoryData = data;
        uint256[] memory memoryValues = values;
        return (memoryData.length, uint8(memoryData[1]), memoryValues.length, memoryValues[1]);
    }

    function consumeReferences(bytes calldata data, uint256[] calldata values)
        external
        pure
        returns (uint256)
    {
        return data.length + uint8(data[1]) + values.length + values[1];
    }

    function verifyStorageReferenceAbiValues()
        external
        returns (uint256, uint256, uint256, uint256)
    {
        initializeReferenceValues();
        bytes storage data = sourceBytes;
        uint256[] storage values = sourceValues;
        bytes memory encoded = abi.encode(data);
        bytes memory packed = abi.encodePacked(data);
        uint256 consumed = this.consumeReferences(data, values);
        return (
            keccak256(encoded) == keccak256(abi.encode(hex"aabbcc")) ? 1 : 0,
            keccak256(packed) == keccak256(hex"aabbcc") ? 1 : 0,
            keccak256(data) == keccak256(hex"aabbcc") ? 1 : 0,
            consumed
        );
    }

    function returnStorageReferences() internal view returns (bytes memory, uint256[] memory) {
        bytes storage data = sourceBytes;
        uint256[] storage values = sourceValues;
        return (data, values);
    }

    function verifyStorageReferenceReturns()
        external
        returns (uint256, uint256, uint256, uint256)
    {
        initializeReferenceValues();
        (bytes memory data, uint256[] memory values) = returnStorageReferences();
        return (data.length, uint8(data[2]), values.length, values[0]);
    }

    function echo(uint256 value) external pure returns (uint256) {
        return value;
    }

    function verifyStorageReferenceLowLevelCall() external returns (uint256) {
        sourceBytes = abi.encodeCall(this.echo, (7));
        bytes storage data = sourceBytes;
        (bool success, bytes memory returndata) = address(this).call(data);
        require(success);
        return abi.decode(returndata, (uint256));
    }

    function consumeMemoryReferences(bytes memory data, uint256[] memory values)
        internal
        pure
        returns (uint256, uint256, uint256, uint256)
    {
        return (data.length, uint8(data[1]), values.length, values[1]);
    }

    function verifyStorageReferenceInternalCall()
        external
        returns (uint256, uint256, uint256, uint256)
    {
        initializeReferenceValues();
        bytes storage data = sourceBytes;
        uint256[] storage values = sourceValues;
        return consumeMemoryReferences(data, values);
    }

    function verifyStorageReferencePush()
        external
        returns (uint256, uint256, uint256, uint256, uint256)
    {
        bytes4[] memory selectors = new bytes4[](1);
        selectors[0] = bytes4(0x11223344);
        sourceSel = Sel(address(0x1234), selectors);
        Sel storage selection = sourceSel;
        list.push(selection);

        sourceFixed = [uint256(3), 5, 8];
        uint256[3] storage values = sourceFixed;
        fixedLists.push(values);

        return (
            uint160(list[0].addr),
            uint32(list[0].selectors[0]),
            fixedLists[0][0],
            fixedLists[0][1],
            fixedLists[0][2]
        );
    }

    function verifyTupleStorageReferenceDeclaration()
        external
        returns (uint256, uint256, uint256, uint256, uint256)
    {
        initializeReferenceValues();
        bytes storage storageData = sourceBytes;
        uint256[] storage storageValues = sourceValues;
        (bytes memory data, uint256[] memory values, uint256 marker) =
            (storageData, storageValues, 91);
        return (data.length, uint8(data[1]), values.length, values[1], marker);
    }

    function verifyTupleStorageReferenceAssignment()
        external
        returns (uint256, uint256, uint256, uint256, uint256)
    {
        initializeReferenceValues();
        bytes storage storageData = sourceBytes;
        uint256[] storage storageValues = sourceValues;
        bytes memory data;
        uint256[] memory values;
        uint256 marker;
        (data, values, marker) = (storageData, storageValues, 92);
        return (data.length, uint8(data[2]), values.length, values[0], marker);
    }

    function verifyTupleStorageAliasDeclaration() external returns (uint256, uint256) {
        initializeReferenceValues();
        (uint256[] storage values, uint256 marker) = (sourceValues, 93);
        values[1] = 7;
        return (sourceValues[1], marker);
    }

    function verifyTupleStorageAliasAssignment() external returns (uint256, uint256) {
        initializeReferenceValues();
        uint256[] storage values = dynamicValues;
        uint256 marker;
        (values, marker) = (sourceValues, 94);
        values[0] = 8;
        return (sourceValues[0], marker);
    }

    function verifyStorageReferenceMemoryAggregates()
        external
        returns (uint256, uint256, uint256, uint256)
    {
        bytes4[] memory selectors = new bytes4[](1);
        selectors[0] = bytes4(0x11223344);
        sourceSel = Sel(address(0x1234), selectors);
        Sel storage selection = sourceSel;
        Sel memory copiedSelection = selection;

        sourceFixed = [uint256(3), 5, 8];
        uint256[3] storage values = sourceFixed;
        uint256[3] memory copiedValues = values;

        return (
            uint160(copiedSelection.addr),
            uint32(copiedSelection.selectors[0]),
            copiedValues[0],
            copiedValues[2]
        );
    }

    function verifyParenthesizedTupleStorageAliasAssignment() external returns (uint256, uint256) {
        initializeReferenceValues();
        uint256[] storage values = dynamicValues;
        uint256 marker;
        ((values), marker) = (sourceValues, 95);
        values[0] = 9;
        return (sourceValues[0], marker);
    }

    function returnStorageReference()
        internal
        view
        returns (uint256[] storage values, uint256 marker)
    {
        return (sourceValues, 96);
    }

    function verifyReturnedStorageReferenceDeclaration() external returns (uint256, uint256) {
        initializeReferenceValues();
        (uint256[] memory values, uint256 marker) = returnStorageReference();
        return (values[1], marker);
    }

    function verifyReturnedStorageReferenceAssignment() external returns (uint256, uint256) {
        initializeReferenceValues();
        uint256[] memory values;
        uint256 marker;
        (values, marker) = returnStorageReference();
        return (values[1], marker);
    }

    function verifyReturnedStorageAlias() external returns (uint256, uint256) {
        initializeReferenceValues();
        (uint256[] storage values, uint256 marker) = returnStorageReference();
        values[0] = 10;
        return (sourceValues[0], marker);
    }

    function namedTernaryStorageReference(bool chooseSource)
        internal
        view
        returns (uint256[] storage values, uint256 marker)
    {
        values = chooseSource ? sourceValues : dynamicValues;
        marker = 97;
    }

    function verifyNamedTernaryStorageAlias() external returns (uint256, uint256, uint256) {
        initializeReferenceValues();
        (uint256[] storage values, uint256 marker) = namedTernaryStorageReference(true);
        values[0] = 11;
        return (values[1], sourceValues[0], marker);
    }

    function ternaryTupleStorageReference(bool chooseSource)
        internal
        view
        returns (uint256[] storage, uint256)
    {
        return chooseSource ? (sourceValues, 98) : (dynamicValues, 99);
    }

    function verifyTernaryTupleStorageAlias() external returns (uint256, uint256, uint256) {
        initializeReferenceValues();
        (uint256[] storage values, uint256 marker) = ternaryTupleStorageReference(true);
        values[0] = 12;
        return (values[1], sourceValues[0], marker);
    }
}
