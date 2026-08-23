//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f() => 16
// ported-from: test/libsolidity/semanticTests/constructor/constructor_function_complex.sol

contract Target {
    uint256 public value;

    constructor(function() external pure returns (uint256) callback) {
        value = callback();
    }
}

contract Caller {
    function f() external returns (uint256) {
        Target target = new Target(this.sixteen);
        return target.value();
    }

    function sixteen() external pure returns (uint256) {
        return 16;
    }
}
