// ported-from: test/libsolidity/syntaxTests/types/functionTypes/selector/state_variable_selector_not_pure.sol
contract A {
    function() external public f;
}
contract B {
    function() external public g;
}

contract C is B {
    function() external public h;
    bytes4 constant s1 = h.selector; //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes4 constant s2 = B.g.selector; //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes4 constant s3 = this.h.selector; //~ ERROR: initial value for constant variable has to be compile-time constant
}
