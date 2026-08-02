//@ run-call: TryCatchReturn::success() => 13
//@ run-call: TryCatchReturn::failure() => 9

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
}
