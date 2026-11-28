//@ codegen-matrix: standard parisGas parisSize
//@ compile-flags: --allow=0067
//@[parisGas] compile-flags: --evm-version paris -O gas
//@[parisSize] compile-flags: --evm-version paris -O size
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: unsignedWords [] => 0
//@ run-call: unsignedWords [1, 2] => 0
//@ run-call: signedWords [-1, 2] => 0
//@ run-call: bytesWords [0x0000000000000000000000000000000000000000000000000000000000000001] => 0
//@ run-call: repeated [1, 2] => 0, 7
//@ run-call: laterAllocation [1, 2] => 0, 1, 2
//@ run-call: activeMemory [1, 2] => 64
//@ run-call: activeMemory [] => 0
//@ run-call: localActiveMemory => 64
//@ run-call: narrowWords [1, 255] => 0
//@ run-call: fixedRows [[1, 2], [3, 4]] => 0
//@ run-call: enumWords [0, 2] => 0
//@ run-call: functionWords => 0
//@ run-call: dirtyNarrow => 0, 511
//@ run-call: nestedDynamic [[1, 2], [], [3]] => 1
//@ run-call: internalThenAllocate [1, 2] => 0, 64, 0, 1
//@ run-call: explicitEncoding [1, 2] => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0, 96

// CHECK: @module

// Indexed topics stage their encoding without a length header or an FMP bump, as solc does.
// The payload is still copied, so an MSIZE observation sees the staged memory.
contract IndexedWordArrayStaging {
    enum Choice { Zero, One, Two }
    event Unsigned(uint256[] indexed values);
    event Signed(int256[] indexed values);
    event BytesWords(bytes32[] indexed values);
    event Narrow(uint8[] indexed values);
    event Rows(uint256[2][] indexed values);
    event Choices(Choice[] indexed values);
    event Callbacks(function() external[] indexed values);
    event Nested(uint256[][] indexed values);

    function unsignedWords(uint256[] memory values) external returns (uint256) {
        uint256 before = freePointer();
        emit Unsigned(values);
        return freePointer() - before;
    }

    function signedWords(int256[] memory values) external returns (uint256) {
        uint256 before = freePointer();
        emit Signed(values);
        return freePointer() - before;
    }

    function bytesWords(bytes32[] memory values) external returns (uint256) {
        uint256 before = freePointer();
        emit BytesWords(values);
        return freePointer() - before;
    }

    function repeated(uint256[] memory values) external returns (uint256, uint256) {
        uint256 before = freePointer();
        emit Unsigned(values);
        values[0] = 7;
        emit Unsigned(values);
        return (freePointer() - before, values[0]);
    }

    function laterAllocation(uint256[] memory values)
        external returns (uint256 beforeAllocation, uint256 first, uint256 last)
    {
        uint256 before = freePointer();
        emit Unsigned(values);
        beforeAllocation = freePointer() - before;
        uint256[] memory later = new uint256[](2);
        later[0] = 99;
        first = values[0];
        last = values[1];
        require(later[0] == 99);
    }

    function activeMemory(uint256[] memory values) external returns (uint256) {
        uint256 before;
        uint256 after_;
        assembly ("memory-safe") { before := msize() }
        emit Unsigned(values);
        assembly ("memory-safe") { after_ := msize() }
        return after_ - before;
    }

    function explicitEncoding(uint256[] memory values)
        external pure returns (bytes32 digest, uint256 reserved)
    {
        uint256 before = freePointer();
        digest = keccak256(abi.encodePacked(values));
        reserved = freePointer() - before;
    }

    function localActiveMemory() external returns (uint256) {
        uint256[] memory values = new uint256[](2);
        values[0] = 1;
        values[1] = 2;
        uint256 before;
        uint256 after_;
        assembly ("memory-safe") { before := msize() }
        emit Unsigned(values);
        assembly ("memory-safe") { after_ := msize() }
        return after_ - before;
    }

    function narrowWords(uint8[] memory values) external returns (uint256) {
        uint256 before = freePointer();
        emit Narrow(values);
        return freePointer() - before;
    }

    function fixedRows(uint256[2][] memory values) external returns (uint256) {
        uint256 before = freePointer();
        emit Rows(values);
        return freePointer() - before;
    }

    function enumWords(Choice[] memory values) external returns (uint256) {
        uint256 before = freePointer();
        emit Choices(values);
        return freePointer() - before;
    }

    function functionWords() external returns (uint256) {
        function() external[] memory callbacks = new function() external[](1);
        callbacks[0] = IndexedWordArrayStaging(address(0)).target;
        uint256 before = freePointer();
        emit Callbacks(callbacks);
        return freePointer() - before;
    }

    function target() external {}

    function dirtyNarrow() external returns (uint256 growth, uint256 raw) {
        uint8[] memory values = new uint8[](1);
        assembly ("memory-safe") { mstore(add(values, 32), 511) }
        uint256 before = freePointer();
        emit Narrow(values);
        growth = freePointer() - before;
        assembly ("memory-safe") { raw := mload(add(values, 32)) }
    }

    function nestedDynamic(uint256[][] memory values) external returns (uint256) {
        emit Nested(values);
        return values[0][0];
    }

    function internalThenAllocate(uint256[] memory values)
        external returns (uint256 growth, uint256 expanded, uint256 zero, uint256 first)
    {
        uint256 before = freePointer();
        assembly ("memory-safe") { expanded := msize() }
        inner(values);
        assembly ("memory-safe") { expanded := sub(msize(), expanded) }
        growth = freePointer() - before;
        uint256[] memory later = new uint256[](values.length);
        zero = later[0];
        first = values[0];
    }

    /// @custom:solar-inline never
    function inner(uint256[] memory values) private {
        emit Unsigned(values);
    }

    function freePointer() private pure returns (uint256 pointer) {
        assembly ("memory-safe") { pointer := mload(0x40) }
    }
}
