//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: success => 15
//@ run-call: failure => 9

contract TryMultiReturnTarget {
    function value(bool ok) external pure returns (uint256, string memory) {
        if (!ok) revert("no");
        return (13, "ok");
    }
}

contract TryMultiReturn {
    TryMultiReturnTarget private target;

    constructor() {
        target = new TryMultiReturnTarget();
    }

    function success() external view returns (uint256) {
        try target.value(true) returns (uint256 value, string memory text) {
            return value + bytes(text).length;
        } catch {
            return 0;
        }
    }

    function failure() external view returns (uint256) {
        try target.value(false) returns (uint256, string memory) {
            return 0;
        } catch {
            return 9;
        }
    }
}
