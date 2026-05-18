//@compile-flags: -Ztypeck
// Ported from test/libsolidity/syntaxTests/immutable/variable_declaration_value.sol.
// Ported from test/libsolidity/semanticTests/immutable/multiple_initializations.sol.
// Ported from test/libsolidity/syntaxTests/immutable/ctor_initialization_tuple.sol.
// Ported from test/libsolidity/syntaxTests/immutable/inheritance_ctor_argument.sol.
// Ported from test/libsolidity/syntaxTests/immutable/writing_after_initialization.sol.
// Ported from test/libsolidity/syntaxTests/immutable/ctor_indirect_initialization.sol.

contract Base {
    constructor(uint256) {}
}

contract Test is Base {
    uint256 immutable INLINE_ASSIGN = INLINE_ASSIGN = 1;
    uint256 immutable STATE_INIT;
    uint256 immutable IMMUT;
    uint256 immutable OTHER;
    uint256 immutable VIA_MODIFIER;
    uint256 immutable VIA_BASE;
    uint256 state = STATE_INIT = 5;

    modifier init(uint256 value) {
        value;
        _;
    }

    constructor() Base(VIA_BASE = 9) init(VIA_MODIFIER = 6) {
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
