//@compile-flags: -Ztypeck

contract Test {
    uint256 immutable IMMUT;
    uint256 immutable OTHER;
    uint256 immutable VIA_MODIFIER;

    modifier init(uint256 value) {
        value;
        _;
    }

    constructor() init(VIA_MODIFIER = 6) {
        uint256 two = 2;
        uint256 three = 3;
        IMMUT = 1;
        (IMMUT, OTHER) = (two, three);
        IMMUT += 4;
        IMMUT++;
        delete OTHER;
    }

    function test() external {
        IMMUT = 7; //~ ERROR: cannot assign to an immutable variable
    }

    function indirect() internal {
        IMMUT = 8; //~ ERROR: cannot assign to an immutable variable
    }
}
