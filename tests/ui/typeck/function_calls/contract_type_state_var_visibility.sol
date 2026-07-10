//@ compile-flags: -Ztypeck

contract StateVarScope {
    uint256 stateVar = 42;
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
