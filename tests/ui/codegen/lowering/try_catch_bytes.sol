//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
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
