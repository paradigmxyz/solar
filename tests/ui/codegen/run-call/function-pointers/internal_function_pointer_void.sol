//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
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
