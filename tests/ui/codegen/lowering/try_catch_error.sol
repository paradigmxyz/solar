//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: TryCatchError::success => 7
//@ run-call: TryCatchError::failure => 2

contract TryCatchErrorTarget {
    function ok() external pure {}

    function fail() external pure {
        revert("no");
    }
}

contract TryCatchError {
    TryCatchErrorTarget private target;

    constructor() {
        target = new TryCatchErrorTarget();
    }

    function success() external view returns (uint256) {
        try target.ok() {} catch Error(string memory reason) {
            return bytes(reason).length;
        }
        return 7;
    }

    function failure() external view returns (uint256) {
        try target.fail() {} catch Error(string memory reason) {
            return bytes(reason).length;
        }
        return 0;
    }
}
