//@ codegen-matrix: standard parisGas parisSize
//@ compile-flags: --allow=0067
//@[parisGas] compile-flags: --evm-version paris -O gas
//@[parisSize] compile-flags: --evm-version paris -O size
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: memoryPair [], [] => 0, 5
//@ run-call: memoryPair [1, 2], [3] => 0, 5
//@ run-call: memoryPair [1, 2, 3], [4, 5] => 0, 5
//@ run-call: calldataPair [], [] => 0, 0
//@ run-call: calldataPair [1, 2], [0, 255] => 0, 4
//@ run-call: calldataPair [9], [7] => 0, 2
//@ run-call: reuseScratch [1, 2] => 0, 199
//@ run-call: preserveInput [1, 2] => 0, 1, 2
//@ run-call: sequential [1, 2], [3, 4] => 0, 4

pragma solidity ^0.8.0;

// CHECK: @module

// Regression probe for several indexed array hashes sharing the same ephemeral
// memory, and for allocations performed after those topics have been emitted.
// Outputs alone are NOT sufficient to certify topic equivalence: the external
// transaction oracle must also compare receipt logs against solc per test.
contract IndexedArrayMultiTopicScratch {
    event Pair(uint256[] indexed left, uint256[] indexed right);
    event Mixed(uint256[] indexed words, uint8[] indexed flags);
    event Words(uint256[] indexed words);

    function memoryPair(uint256[] memory left, uint256[] memory right)
        external returns (uint256 growth, uint256 afterAllocationLength)
    {
        uint256 before = freePointer();
        emit Pair(left, right);
        growth = freePointer() - before;
        bytes memory later = new bytes(5);
        afterAllocationLength = later.length;
    }

    function calldataPair(uint256[] calldata words, uint8[] calldata flags)
        external returns (uint256 growth, uint256 combinedLength)
    {
        uint256 before = freePointer();
        emit Mixed(words, flags);
        growth = freePointer() - before;
        combinedLength = words.length + flags.length;
    }

    function reuseScratch(uint256[] calldata values)
        external returns (uint256 growth, uint256 initialized)
    {
        uint256 before = freePointer();
        emit Words(values);
        emit Words(values);
        growth = freePointer() - before;
        uint256[] memory later = new uint256[](2);
        later[0] = 99;
        later[1] = 100;
        initialized = later[0] + later[1];
    }

    function preserveInput(uint256[] memory values)
        external returns (uint256 growth, uint256 first, uint256 second)
    {
        uint256 before = freePointer();
        emit Words(values);
        growth = freePointer() - before;
        first = values[0];
        second = values[1];
    }

    function sequential(uint256[] calldata words, uint256[] calldata others)
        external returns (uint256 growth, uint256 combinedLength)
    {
        uint256 before = freePointer();
        emit Pair(words, others);
        emit Words(others);
        emit Words(words);
        growth = freePointer() - before;
        combinedLength = words.length + others.length;
    }

    function freePointer() private pure returns (uint256 result) {
        assembly ("memory-safe") { result := mload(0x40) }
    }
}
