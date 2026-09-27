uint256 constant N_COINS = 2;

contract Base {
    function privateFunction() private pure returns (uint256) { return 1; }
    function externalFunction() external pure returns (uint256) { return 2; }
    function N_COINS() external pure returns (uint256) { return 3; }
    function value() public pure returns (uint256) { return N_COINS; }
    function assemblyValue() public pure returns (uint256 result) {
        assembly { result := N_COINS }
    }

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

interface StableSwap {
    function N_COINS() external view returns (uint256);
    function balances() external view returns (uint256[N_COINS] memory);
}
