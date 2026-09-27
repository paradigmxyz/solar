//@ filecheck:
// CHECK: @module
// CHECK-NOT: icall returndata_bytes<>
//@ codegen-matrix: standard
//@ run-call: highLevelGas => true
//@ run-call: TryCallOptions::discardOptions => 2
//@ run-call: TryCallOptions::optionMembers => true
//@ run-call: BareCatchReturndata::read => 42

contract TryCallOptionsTarget {
    function ping() external {}
}

contract TryCallOptions {
    TryCallOptionsTarget private target;

    constructor() {
        target = new TryCallOptionsTarget();
    }

    uint private effects;

    function optionValue() internal returns (uint) { effects++; return 0; }

    function discardOptions() external returns (uint) {
        address(target).call{gas: optionValue(), value: optionValue()};
        return effects;
    }

    function optionMembers() external returns (bool) {
        return target.ping{gas: optionValue()}.address == address(target)
            && target.ping{gas: optionValue()}.selector == target.ping.selector
            && effects == 2;
    }

    function highLevelGas() external returns (bool) {
        try target.ping{gas: 0}() {
            return false;
        } catch {
            return true;
        }
    }
}

contract BareCatchReturndata {
    function fail() external pure {
        assembly {
            mstore(0, 42)
            revert(0, 32)
        }
    }

    function read() external view returns (uint256 result) {
        try this.fail() {} catch {
            assembly {
                returndatacopy(0, 0, 32)
                result := mload(0)
            }
        }
    }
}
