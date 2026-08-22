//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: reasonLength() => 5

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
