contract Base {
    function privateFunction() private pure returns (uint256) { return 1; }
    function externalFunction() external pure returns (uint256) { return 2; }
}

contract Derived is Base {
    function callPrivate() public pure returns (uint256) {
        return privateFunction(); //~ ERROR: unresolved symbol `privateFunction`
    }

    function callExternal() public pure returns (uint256) {
        return externalFunction(); //~ ERROR: unresolved symbol `externalFunction`
    }
}

contract Indirect is Derived {
    function callIndirectPrivate() public pure returns (uint256) {
        return privateFunction(); //~ ERROR: unresolved symbol `privateFunction`
    }

    function callIndirectExternal() public pure returns (uint256) {
        return externalFunction(); //~ ERROR: unresolved symbol `externalFunction`
    }
}
