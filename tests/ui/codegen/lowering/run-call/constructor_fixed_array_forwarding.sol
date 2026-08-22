//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: C::a(); constructor=[1, [2, 3, 4]] => 1
//@ run-call: C::b(uint256) 0; constructor=[1, [2, 3, 4]] => 2
//@ run-call: C::b(uint256) 1; constructor=[1, [2, 3, 4]] => 3
//@ run-call: C::b(uint256) 2; constructor=[1, [2, 3, 4]] => 4
// ported-from: test/libsolidity/semanticTests/constructor/constructor_static_array_argument.sol

contract C {
    uint256 public a;
    uint256[3] public b;

    constructor(uint256 _a, uint256[3] memory _b) {
        a = _a;
        b = _b;
    }
}
