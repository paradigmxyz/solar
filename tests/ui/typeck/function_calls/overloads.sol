//@compile-flags: -Ztypeck

contract C {
    event E(uint8 small);
    event E(uint256 wide);

    function pick(uint8 small) internal pure returns (uint8) {
        return small;
    }

    function pick(uint256 wide) internal pure returns (uint256) {
        return wide;
    }

    function named(uint8 small) internal pure returns (uint8) {
        return small;
    }

    function named(uint256 wide) internal pure returns (uint256) {
        return wide;
    }

    function ok(uint8 small, uint256 wide) public {
        uint8 a = pick(small);
        uint256 b = pick(wide);
        uint8 c = named({small: small});
        uint256 d = named({wide: wide});
        emit E(small);
        emit E(wide);
    }

    function ambiguous() public pure {
        pick(1); //~ ERROR: no unique declarations found
    }

    function noMatch(bool value) public pure {
        pick(value); //~ ERROR: no matching declarations found
    }
}
