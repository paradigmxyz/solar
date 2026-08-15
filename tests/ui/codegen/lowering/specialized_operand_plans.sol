//@ revisions: default specialized
//@[default] compile-flags: -Zevm-no-specialized-storage-operand-plans
//@[default] run-call: sstorePreserves 13 => 40
//@[specialized] run-call: sstorePreserves 13 => 40
//@[default] run-call: tstorePreserves 17 => 52
//@[specialized] run-call: tstorePreserves 17 => 52

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

}
