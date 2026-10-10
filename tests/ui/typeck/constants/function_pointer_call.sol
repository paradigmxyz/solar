// ported-from: test/libsolidity/syntaxTests/constants/initialization/function_pointer_call.sol
contract C {
    function () pure returns (uint) x;
    uint constant y = x(); //~ ERROR: initial value for constant variable has to be compile-time constant
}
