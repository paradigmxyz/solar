//@ filecheck:
// CHECK: @module
// CHECK-NOT: icall returndata_bytes<>
//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ run-call: highLevelGas => true
//@ run-call: TryCallOptions::discardOptions => 2
//@ run-call: TryCallOptions::optionMembers => true
//@ run-call: TryCallOptions::directSelector => true
//@ run-call: TryCallOptions::pointerSelector true => true
//@ run-call: TryCallOptions::pointerSelector false => true
//@ run-call: TryCallOptions::returnedSelector => true
//@ run-call: BareCatchReturndata::read => 42

contract TryCallOptionsTarget {
    function ping() external {}
    function payablePing() external payable { revert(); }
    function payablePong() external payable { revert(); }
}

contract TryCallOptions {
    TryCallOptionsTarget private target;

    constructor() {
        target = new TryCallOptionsTarget();
    }

    uint private effects;
    uint private receiverEffects;

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

    function selectorOption() internal returns (uint) { effects++; return 1; }

    function receiver() internal returns (TryCallOptionsTarget) {
        receiverEffects++;
        return target;
    }

    function functionValue() internal returns (function() external payable) {
        receiverEffects++;
        return target.payablePing;
    }

    function directSelector() external returns (bool) {
        bytes4 selector = receiver().payablePing{value: selectorOption(), gas: selectorOption()}.selector;
        return selector == bytes4(keccak256("payablePing()")) && effects == 2 && receiverEffects == 1;
    }

    function pointerSelector(bool alternate) external returns (bool) {
        function() external payable pointer = alternate ? target.payablePing : target.payablePong;
        bytes4 selector = (pointer{gas: selectorOption(), value: selectorOption()}).selector;
        bytes4 expected = alternate ? bytes4(keccak256("payablePing()")) : bytes4(keccak256("payablePong()"));
        return selector == expected && effects == 2 && receiverEffects == 0;
    }

    function returnedSelector() external returns (bool) {
        bytes4 selector = functionValue(){gas: selectorOption(), value: selectorOption()}.selector;
        return selector == bytes4(keccak256("payablePing()")) && effects == 2 && receiverEffects == 1;
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
