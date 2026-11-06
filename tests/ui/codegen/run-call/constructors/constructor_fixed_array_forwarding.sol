//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: C::a; constructor=[1, [2, 3, 4]] => 1
//@ run-call: C::b 0; constructor=[1, [2, 3, 4]] => 2
//@ run-call: C::b 1; constructor=[1, [2, 3, 4]] => 3
//@ run-call: C::b 2; constructor=[1, [2, 3, 4]] => 4
// ported-from: test/libsolidity/semanticTests/constructor/constructor_static_array_argument.sol

contract C {
    uint256 public a;
    uint256[3] public b;

    constructor(uint256 _a, uint256[3] memory _b) {
        a = _a;
        b = _b;
    }
}
