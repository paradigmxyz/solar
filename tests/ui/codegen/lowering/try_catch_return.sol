//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: TryCatchReturn::success() => 13
//@ run-call: TryCatchReturn::failure() => 9
//@ run-call: TryCatchReturn::ignored() => 13

contract TryCatchReturnTarget {
    function value(bool ok) external pure returns (uint256) {
        if (!ok) revert();
        return 13;
    }
}

contract TryCatchReturn {
    TryCatchReturnTarget private target;

    constructor() {
        target = new TryCatchReturnTarget();
    }

    function success() external view returns (uint256 result) {
        try target.value(true) returns (uint256 value) {
            result = value;
        } catch {
            result = 9;
        }
    }

    function failure() external view returns (uint256 result) {
        try target.value(false) returns (uint256 value) {
            result = value;
        } catch {
            result = 9;
        }
    }

    function ignored() external view returns (uint256) {
        try target.value(true) {
            return 13;
        } catch {
            return 9;
        }
    }
}
