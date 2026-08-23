//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: TryCatchBytes::success() => 7
//@ run-call: TryCatchBytes::failure() => 100

contract TryCatchBytesTarget {
    function ok() external pure {}

    function fail() external pure {
        revert("no");
    }
}

contract TryCatchBytes {
    TryCatchBytesTarget private target;

    constructor() {
        target = new TryCatchBytesTarget();
    }

    function success() external view returns (uint256) {
        try target.ok() {} catch (bytes memory reason) {
            return reason.length;
        }
        return 7;
    }

    function failure() external view returns (uint256) {
        try target.fail() {} catch (bytes memory reason) {
            return reason.length;
        }
        return 0;
    }
}
