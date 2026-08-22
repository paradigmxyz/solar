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
//@[none, gas, size] run-call: f() => 0
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_clear_storage.sol

contract ArrayCopyClearStorage {
    uint256[] x;

    function f() public returns (uint256) {
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        uint256[] memory y = new uint256[](1);
        y[0] = 23;
        x = y;
        assembly {
            sstore(x.slot, 4)
        }
        assert(x[1] == 0);
        assert(x[2] == 0);
        return x[3];
    }
}
