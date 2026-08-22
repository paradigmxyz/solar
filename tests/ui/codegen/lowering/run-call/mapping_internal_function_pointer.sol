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
//@[none, gas, size] run-call: run() => true
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
