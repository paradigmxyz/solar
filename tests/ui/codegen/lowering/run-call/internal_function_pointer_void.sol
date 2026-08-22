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
//@[none, gas, size] run-call: f => true
// ported-from: test/libsolidity/semanticTests/functionCall/call_function_returning_nothing_via_pointer.sol

contract InternalFunctionPointerVoid {
    bool private flag;

    function setFlag() internal {
        flag = true;
    }

    function f() external returns (bool) {
        function() internal callback = setFlag;
        callback();
        return flag;
    }
}
