//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: TryCatchPanic::success() => 7
//@[gas] run-call: TryCatchPanic::success() => 7
//@[size] run-call: TryCatchPanic::success() => 7
//@[none] run-call: TryCatchPanic::failure() => 1
//@[gas] run-call: TryCatchPanic::failure() => 1
//@[size] run-call: TryCatchPanic::failure() => 1

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
