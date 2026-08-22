//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f(uint256) 7 => 8
//@ run-call: f2(uint256) 7 => 8
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
