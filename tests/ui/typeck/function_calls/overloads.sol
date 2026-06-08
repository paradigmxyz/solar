//@ compile-flags: -Ztypeck
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
