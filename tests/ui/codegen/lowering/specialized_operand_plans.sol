//@ run-call: sstorePreserves 13 => 40
//@ run-call: tstorePreserves 17 => 52
//@ run-call: mstorePreserves 7, 512 => 22
//@ run-call: SpecializedOperandPlans::highLevelMemory 11 => 34
//@ run-call: CompilerManagedMemory::highLevelMemory 11 => 34

contract SpecializedOperandPlans {
    uint256 private stored;

    function sstorePreserves(uint256 x) external returns (uint256) {
        uint256 value = x * 2 + 1;
        assembly {
            sstore(0, value)
        }
        return value + x;
    }

    function tstorePreserves(uint256 x) external returns (uint256) {
        uint256 value = x * 2 + 1;
        assembly {
            tstore(0, value)
        }
        return value + x;
    }

    function mstorePreserves(uint256 x, uint256 pointer) external pure returns (uint256) {
        uint256 value = x * 2 + 1;
        assembly {
            mstore(pointer, value)
        }
        return value + x;
    }

    function highLevelMemory(uint256 x) external pure returns (uint256) {
        uint256[] memory values = new uint256[](1);
        values[0] = x * 2 + 1;
        return values[0] + x;
    }
}

contract CompilerManagedMemory {
    function highLevelMemory(uint256 x) external pure returns (uint256) {
        uint256[] memory values = new uint256[](1);
        values[0] = x * 2 + 1;
        return values[0] + x;
    }
}
