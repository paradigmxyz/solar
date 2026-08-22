//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f() => true

contract ExternalFunctionPointerAddress {
    function g() external {}

    function f() external view returns (bool) {
        function() external fp = this.g;
        return fp.address == address(this);
    }
}
