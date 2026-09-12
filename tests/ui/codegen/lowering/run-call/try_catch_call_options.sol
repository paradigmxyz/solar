//@ filecheck:
// CHECK: @module
// CHECK-NOT: returndata_bytes
//@ codegen-matrix: standard
//@ run-call: highLevelGas => true
//@ run-call: BareCatchReturndata::read => 42

contract TryCallOptionsTarget {
    function ping() external {}
}

contract TryCallOptions {
    TryCallOptionsTarget private target;

    constructor() {
        target = new TryCallOptionsTarget();
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
