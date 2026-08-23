//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: result() => 4
//@ run-call: use(uint256) 3 => 6
// ported-from: test/libsolidity/semanticTests/constructor/store_function_in_constructor.sol

contract ConstructorStoreFunction {
    uint256 public result;
    function(uint256) internal returns (uint256) callback;

    constructor() {
        callback = double;
        result = use(2);
    }

    function double(uint256 value) public pure returns (uint256) {
        return value * 2;
    }

    function use(uint256 value) public returns (uint256) {
        return callback(value);
    }
}
