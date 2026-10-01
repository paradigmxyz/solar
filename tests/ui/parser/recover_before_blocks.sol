//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

// Recovery must retain both the following block and the statement after it.
// Each type error below proves that its statement reached semantic analysis.
function plus(uint256 value, uint256 amount) pure returns (uint256) {
    return value + amount;
}

contract C {
    using {plus} for uint256;
    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    function memberBeforeIf(uint256 x) internal pure {
        x.
        if (true) { //~ ERROR: expected identifier
            //~^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function compoundMemberBeforeFor(uint256 x) internal pure {
        (x + 1).
        for (;;) { //~[recover] ERROR: expected identifier
            //~[recover]^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
            break;
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function callBeforeWhile() internal pure {
        add( //~[recover] ERROR: wrong argument count
        while (false) { //~[recover] ERROR: expected `)`
            //~[recover]^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function argumentBeforeUnchecked() internal pure {
        add(1 //~[recover] ERROR: wrong argument count
        unchecked { //~[recover] ERROR: expected `)`
            //~[recover]^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function commaBeforeBlock() internal pure {
        add(1, //~[recover] ERROR: wrong argument count
        { //~[recover] ERROR: expected one of
            //~[recover]^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function nestedCallBeforeIf() internal pure {
        add(add(1 //~[recover] ERROR: wrong argument count
        //~[recover]^ ERROR: wrong argument count
        if (true) { //~[recover] ERROR: expected `)`
            //~[recover]^ ERROR: expected `)`
            //~[recover]^^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function memberCallBeforeFor(uint256 x) internal pure {
        x.plus( //~[recover] ERROR: wrong argument count
        for (;;) { //~[recover] ERROR: expected `)`
            //~[recover]^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
            break;
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function callBeforeBlock() internal pure {
        add( //~[recover] ERROR: wrong argument count
        { //~[recover] ERROR: expected `)`
            //~[recover]^ ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function callBeforeEmptyBlock() internal pure {
        add( //~[recover] ERROR: wrong argument count
        {} //~[recover] ERROR: expected `)`
        //~[recover]^ ERROR: expected one of
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }

    function closedCallBeforeUnchecked() internal pure {
        add(1, 2)
        unchecked { //~[recover] ERROR: expected one of
            uint8 inside = 300; //~[recover] ERROR: mismatched types
        }
        uint8 afterBlock = 300; //~[recover] ERROR: mismatched types
    }
}
