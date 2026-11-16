//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f 7 => 8
//@ run-call: f2 7 => 8
// ported-from: test/libsolidity/semanticTests/functionTypes/pass_function_types_externally.sol

contract ExternalFunctionPointerParameter {
    function f(uint256 value) public returns (uint256) {
        return this.eval(this.increment, value);
    }

    function f2(uint256 value) public returns (uint256) {
        return eval(this.increment, value);
    }

    function eval(function(uint256) external returns (uint256) callback, uint256 value)
        public
        returns (uint256)
    {
        return callback(value);
    }

    function increment(uint256 value) public pure returns (uint256) {
        return value + 1;
    }
}
