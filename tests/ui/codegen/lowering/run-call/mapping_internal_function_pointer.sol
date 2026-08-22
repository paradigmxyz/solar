//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: run() => true
// ported-from: test/libsolidity/semanticTests/functionTypes/mapping_of_functions.sol

contract MappingInternalFunctionPointer {
    address private constant KEY = address(0x1234);
    bool private success;
    mapping(address => function() internal) stages;

    constructor() {
        stages[KEY] = stage0;
    }

    function stage0() internal {
        stages[KEY] = stage1;
    }

    function stage1() internal {
        stages[KEY] = stage2;
    }

    function stage2() internal {
        success = true;
    }

    function run() external returns (bool) {
        stages[KEY]();
        stages[KEY]();
        stages[KEY]();
        return success;
    }
}
