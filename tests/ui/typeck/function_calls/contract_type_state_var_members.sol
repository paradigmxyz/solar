//@ compile-flags: -Ztypeck

// ported-from: test/libsolidity/semanticTests/various/state_variable_under_contract_name.sol

contract StateVarScope {
    uint256 stateVar = 42;

    function getStateVar() public view returns (uint256 value) {
        value = StateVarScope.stateVar;
    }
}

contract OtherScope {
    function getStateVar() public view returns (uint256 value) {
        value = StateVarScope.stateVar; //~ ERROR: member `stateVar` not found
    }
}

contract PublicStateVarScope {
    uint256 public stateVar = 42;

    function getStateVar() public view returns (uint256 value) {
        value = PublicStateVarScope.stateVar;
    }
}
