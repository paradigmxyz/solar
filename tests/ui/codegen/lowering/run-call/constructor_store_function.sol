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
//@[none] run-call: result() => 4
//@[gas] run-call: result() => 4
//@[size] run-call: result() => 4
//@[none] run-call: use(uint256) 3 => 6
//@[gas] run-call: use(uint256) 3 => 6
//@[size] run-call: use(uint256) 3 => 6
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
