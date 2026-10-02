//@ codegen-matrix: standard
//@ run-call: disjoint() => 20
//@ run-call: overlap() => 17
//@ run-call: unknown 0 => 17
//@ run-call: unknown 1 => 20
//@ run-call: recursive 2 => 20
//@ run-call: transientSlots() => 20
//@ run-call: observedReturn() => 7
//@ run-call-fail: observedRevert() => 0x0000000000000000000000000000000000000000000000000000000000000007

//@ run-call: memoryDisjoint() => 20
//@ run-call: memoryOverlap() => 17
//@ run-call: memoryPartialOverlap() => 10
//@ run-call: memoryRecursive() => 17

contract InternalCallEffects {
    function disjoint() external returns (uint256) {
        assembly { sstore(0, 10) }
        uint256 before;
        assembly { before := sload(0) }
        writeOne();
        uint256 afterValue;
        assembly { afterValue := sload(0) }
        return before + afterValue;
    }

    function overlap() external returns (uint256) {
        assembly { sstore(1, 10) }
        uint256 before;
        assembly { before := sload(1) }
        writeOne();
        uint256 afterValue;
        assembly { afterValue := sload(1) }
        return before + afterValue;
    }

    function unknown(uint256 slot) external returns (uint256) {
        assembly { sstore(0, 10) }
        uint256 before;
        assembly { before := sload(0) }
        writeSlot(slot);
        uint256 afterValue;
        assembly { afterValue := sload(0) }
        return before + afterValue;
    }

    function recursive(uint256 depth) external returns (uint256) {
        assembly { sstore(0, 10) }
        uint256 before;
        assembly { before := sload(0) }
        recurse(depth);
        uint256 afterValue;
        assembly { afterValue := sload(0) }
        return before + afterValue;
    }

    function transientSlots() external returns (uint256) {
        assembly { tstore(0, 10) }
        uint256 before;
        assembly { before := tload(0) }
        writeTransientOne();
        uint256 afterValue;
        assembly { afterValue := tload(0) }
        return before + afterValue;
    }

    function observedReturn() external pure returns (uint256) {
        assembly { mstore(128, 7) }
        returnMemory();
        assembly { mstore(128, 9) }
        return 9;
    }

    function observedRevert() external pure {
        assembly { mstore(128, 7) }
        revertMemory();
        assembly { mstore(128, 9) }
    }

    function memoryDisjoint() external pure returns (uint256) {
        uint256[2] memory values = [uint256(10), uint256(20)];
        uint256 pointer;
        assembly { pointer := values }
        uint256 before = values[0];
        writeMemory(pointer + 32);
        return before + values[0];
    }

    function memoryOverlap() external pure returns (uint256) {
        uint256[2] memory values = [uint256(10), uint256(20)];
        uint256 pointer;
        assembly { pointer := values }
        uint256 before = values[0];
        writeMemory(pointer);
        return before + values[0];
    }

    function memoryPartialOverlap() external pure returns (uint256) {
        uint256[2] memory values = [uint256(10), uint256(20)];
        uint256 pointer;
        assembly { pointer := values }
        uint256 before = values[0];
        writeMemory(pointer + 1);
        return before + values[0];
    }

    function memoryRecursive() external pure returns (uint256) {
        uint256[3] memory values = [uint256(10), uint256(20), uint256(30)];
        uint256 pointer;
        assembly { pointer := values }
        uint256 before = values[0];
        writeMemoryRecursive(pointer, 2);
        return before + values[0];
    }

    function writeMemory(uint256 pointer) internal pure {
        assembly { mstore(pointer, 7) }
    }

    function writeMemoryRecursive(uint256 pointer, uint256 depth) internal pure {
        writeMemory(pointer);
        if (depth != 0) writeMemoryRecursive(pointer + 32, depth - 1);
    }

    function writeOne() internal {
        assembly { sstore(1, 7) }
    }

    function writeSlot(uint256 slot) internal {
        assembly { sstore(slot, 7) }
    }

    function recurse(uint256 depth) internal {
        writeOne();
        if (depth != 0) recurse(depth - 1);
    }

    function writeTransientOne() internal {
        assembly { tstore(1, 7) }
    }

    function returnMemory() internal pure {
        assembly { return(128, 32) }
    }

    function revertMemory() internal pure {
        assembly { revert(128, 32) }
    }
}
