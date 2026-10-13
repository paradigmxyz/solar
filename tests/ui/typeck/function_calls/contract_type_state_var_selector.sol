// ported-from: test/libsolidity/syntaxTests/types/functionTypes/selector/state_variable_selector_contract_name.sol

contract A {
    function() external public f;
}

contract C {
    bytes4 constant s4 = A.f.selector; //~ ERROR: member `f` not found
}
