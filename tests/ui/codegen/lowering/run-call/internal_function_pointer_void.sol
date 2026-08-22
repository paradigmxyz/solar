//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f => true
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
