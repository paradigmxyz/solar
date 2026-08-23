//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: reasonLength() => 5

contract TryErrorTarget {
    function fail() external pure {
        revert("hello");
    }
}

contract TryErrorCatch {
    TryErrorTarget private target;

    constructor() {
        target = new TryErrorTarget();
    }

    function reasonLength() external view returns (uint256) {
        try target.fail() {
            return 0;
        } catch Error(string memory reason) {
            return bytes(reason).length;
        } catch {
            return 0;
        }
    }
}
