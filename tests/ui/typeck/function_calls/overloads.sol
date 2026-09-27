// ported-from: test/libsolidity/semanticTests/freeFunctions/overloads.sol

contract C {
    event E(bool flag);
    event E(uint256 wide);

    function pick(bool flag) internal pure returns (bool) {
        return flag;
    }

    function pick(uint256 wide) internal pure returns (uint256) {
        return wide;
    }

    function named(bool flag) internal pure returns (bool) {
        return flag;
    }

    function named(uint256 wide) internal pure returns (uint256) {
        return wide;
    }

    function ambiguousPick(uint8 small) internal pure returns (uint8) {
        return small;
    }

    function ambiguousPick(uint256 wide) internal pure returns (uint256) {
        return wide;
    }

    function ok(bool flag, uint256 wide) public {
        bool a = pick(flag);
        uint256 b = pick(wide);
        bool c = named({flag: flag});
        uint256 d = named({wide: wide});
        emit E(flag);
        emit E(wide);
    }

    function ambiguous(uint8 value) public pure {
        ambiguousPick(value); //~ ERROR: no unique declarations found
    }

    function noMatch(address value) public pure {
        pick(value); //~ ERROR: no matching declarations found
    }
}

contract ExternalOverload {
    function choose(uint256 value) external pure returns (uint256) { return value; }
    function choose(uint8 value) public pure returns (uint8) { return value; }
    function functionValue() internal pure returns (function(uint8) internal pure returns (uint8)) { return choose; }
    function callInternal(uint8 value) public pure returns (uint8) { return choose(value); }
}

uint256 constant N_COINS = 2;
interface StableSwap {
    function N_COINS() external view returns (uint256);
    function balances() external view returns (uint256[N_COINS] memory);
}

contract ExternalConstantShadow {
    function N_COINS() external pure returns (uint256) { return 3; }
    function value() public pure returns (uint256) { return N_COINS; }
    function assemblyValue() public pure returns (uint256 result) {
        assembly { result := N_COINS }
    }
}
