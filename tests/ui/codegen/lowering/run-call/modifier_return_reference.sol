//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => 2, 3
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_return_reference.sol

contract ModifierReturnReference {
    modifier setX(uint256 value) {
        _;
    }

    modifier setY(uint256 value) {
        _;
    }

    function f() public setX(x = 2) setY(y = 3) returns (uint256 x, uint256 y) {}
}
