//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: TryCatchError::success() => 7
//@[none, gas, size] run-call: TryCatchError::failure() => 2

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
