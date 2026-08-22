//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f() => 255, 3
//@[none, gas, size] run-call: g() => 2

contract StorageStructBytesIndex {
    struct S {
        bytes b;
    }

    S s;

    constructor() {
        s.b = hex"010203";
    }

    function f() external returns (uint256, uint256) {
        delete s;
        s.b = hex"010203";
        s.b[0] = bytes1(0xff);
        return (uint8(s.b[0]), s.b.length);
    }

    function g() external view returns (uint256) {
        return uint8(bytes(s.b)[1]);
    }
}
