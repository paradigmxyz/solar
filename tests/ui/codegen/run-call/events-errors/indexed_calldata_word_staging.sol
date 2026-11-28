//@ codegen-matrix: standard parisGas parisSize
//@ compile-flags: --allow=0067
//@[parisGas] compile-flags: --evm-version paris -O gas
//@[parisSize] compile-flags: --evm-version paris -O size
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: unsignedWords [] => 0
//@ run-call: unsignedWords [1, 2] => 0
//@ run-call: addresses [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002] => 0
//@ run-call: narrowWords [1, 255] => 0
//@ run-call: signedNarrow [-128, 127] => 0
//@ run-call: booleans [true, false] => 0
//@ run-call: fixedBytes [0x01020304, 0xffffffff] => 0
//@ run-call: choices [0, 2] => 0
//@ run-call: slice [1, 2, 3] => 0
//@ run-call: slice [1] => 0
//@ run-call: internalThenAllocate [1, 2] => 0, 0, 1
//@ run-call: activeMemory [1, 2] => 0, true
//@ run-call: activeMemory [] => 0, true
//@ run-call: multiple [0x0000000000000000000000000000000000000001], [255], 9 => 0
//@ run-call: evaluationOrder [0x0000000000000000000000000000000000000001], false => 1
//@ run-call: explicitEncoding [1, 2] => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0, 96

//@ run-call-fail: 0x616cb423000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000100 => 0x
//@ run-call-fail: 0x6ecb1bab000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000010000000000000000000000000000000000000000 => 0x
//@ run-call-fail: 0xfc16985a000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002 => 0x
//@ run-call-fail: 0xfe522d4c000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call-fail: 0x5b526dfc000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000003 => 0x
//@ run-call-fail: 0x9061c2b9000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000080 => 0x
//@ run-call-fail: 0x6e3feacc000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call-fail: 0x27a0c3dd0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000010000000000000000000000000000000000000000 => 0x2f985525

// CHECK: @module

// Calldata word arrays are validated before their indexed encoding is staged. No memory
// object is reserved, and later allocations still initialize the overwritten scratch area.
contract IndexedCalldataWordStaging {
    enum Choice { Zero, One, Two }
    event Unsigned(uint256[] indexed values);
    event Addresses(address[] indexed values);
    event Narrow(uint8[] indexed values);
    event SignedNarrow(int8[] indexed values);
    event Booleans(bool[] indexed values);
    event FixedBytes(bytes4[] indexed values);
    event Choices(Choice[] indexed values);
    event Multiple(address[] indexed path, uint8[] indexed flags, uint256 data);
    event WithMarker(address[] indexed path, uint256 marker);
    error LaterArgument();
    uint256 counter;

    function unsignedWords(uint256[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit Unsigned(values);
        return freePointer() - before;
    }

    function addresses(address[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit Addresses(values);
        return freePointer() - before;
    }

    function narrowWords(uint8[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit Narrow(values);
        return freePointer() - before;
    }

    function signedNarrow(int8[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit SignedNarrow(values);
        return freePointer() - before;
    }

    function booleans(bool[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit Booleans(values);
        return freePointer() - before;
    }

    function fixedBytes(bytes4[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit FixedBytes(values);
        return freePointer() - before;
    }

    function choices(Choice[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit Choices(values);
        return freePointer() - before;
    }

    function slice(uint256[] calldata values) external returns (uint256) {
        uint256 before = freePointer();
        emit Unsigned(values[1:]);
        return freePointer() - before;
    }

    function internalThenAllocate(uint256[] calldata values)
        external returns (uint256 growth, uint256 zero, uint256 first)
    {
        uint256 before = freePointer();
        inner(values);
        growth = freePointer() - before;
        uint256[] memory later = new uint256[](values.length);
        zero = later[0];
        first = values[0];
    }

    /// @custom:solar-inline never
    function inner(uint256[] calldata values) private {
        emit Unsigned(values);
    }

    function activeMemory(uint256[] calldata values)
        external returns (uint256 growth, bool expandedAsStaged)
    {
        uint256 pointer = freePointer();
        uint256 before;
        uint256 after_;
        assembly ("memory-safe") { before := msize() }
        emit Unsigned(values);
        assembly ("memory-safe") { after_ := msize() }
        growth = freePointer() - pointer;
        uint256 expected = values.length == 0 ? before : pointer + values.length * 32;
        if (expected < before) expected = before;
        expandedAsStaged = after_ == expected;
    }

    function multiple(address[] calldata path, uint8[] calldata flags, uint256 data)
        external returns (uint256)
    {
        uint256 before = freePointer();
        emit Multiple(path, flags, data);
        return freePointer() - before;
    }

    function explicitEncoding(uint256[] calldata values)
        external pure returns (bytes32 digest, uint256 reserved)
    {
        uint256 before = freePointer();
        digest = keccak256(abi.encodePacked(values));
        reserved = freePointer() - before;
    }

    function evaluationOrder(address[] calldata path, bool fail) external returns (uint256) {
        emit WithMarker(path, marker(fail));
        return counter;
    }

    function marker(bool fail) private returns (uint256) {
        ++counter;
        if (fail) revert LaterArgument();
        return counter;
    }

    function freePointer() private pure returns (uint256 pointer) {
        assembly ("memory-safe") { pointer := mload(0x40) }
    }
}
