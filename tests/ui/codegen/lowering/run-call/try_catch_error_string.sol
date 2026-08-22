//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
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
