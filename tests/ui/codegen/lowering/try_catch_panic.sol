//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: TryCatchPanic::success() => 7
//@ run-call: TryCatchPanic::failure() => 1

contract TryCatchPanicTarget {
    function ok() external pure {}

    function fail() external pure {
        assert(false);
    }
}

contract TryCatchPanic {
    TryCatchPanicTarget private target;

    constructor() {
        target = new TryCatchPanicTarget();
    }

    function success() external view returns (uint256) {
        try target.ok() {} catch Panic(uint256 code) {
            return code;
        }
        return 7;
    }

    function failure() external view returns (uint256) {
        try target.fail() {} catch Panic(uint256 code) {
            return code;
        }
        return 0;
    }
}
