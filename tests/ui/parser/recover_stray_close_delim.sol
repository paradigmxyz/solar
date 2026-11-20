//@compile-flags: -Zrecover-incomplete-input

// Recovery must advance past a token it already reported instead of failing at it again.

contract C {
    function whileLoop() public {
        while (true) {
            if () //~ ERROR: expected one of
        }
    }

    function forLoop() public {
        for (;;) {
            if () //~ ERROR: expected one of
        }
    }

    function doWhile() public {
        do {
            if () //~ ERROR: expected one of
        } while (true);
    }

    function bracket() public {
        while (true) {
            if (] //~ ERROR: expected one of
        }
    }

    function orphanElse() public {
        if () {} else {} //~ ERROR: expected one of
        //~^ ERROR: expected one of
    }

    function mismatchedCall() public {
        f(1, ]; //~ ERROR: expected one of
        //~^ ERROR: expected one of
    }

    function tuple() public {
        (x, (1, } //~ ERROR: expected one of

    function doWithoutWhile() public {
        do x; } //~ ERROR: expected `while`

    function doBreak() public {
        do break } //~ ERROR: expected `;`

    function yul() public {
        assembly {
            pop(] } //~ ERROR: expected one of
            //~^ ERROR: wrong argument count
    }

    function later() public {
        uint8 value = 300; //~ ERROR: mismatched types
    }

    struct S { uint[ } //~ ERROR: expected one of
    //~^ ERROR: structs must have at least one field
}  //~ ERROR: expected `;`

contract Unfinished {
    function f(uint256 x, } //~ ERROR: expected one of

contract Later {
    uint8 value = 300; //~ ERROR: mismatched types
}

contract D {
    uint stateVar = f(1, ]; //~ ERROR: expected one of
    //~^ ERROR: expected global item
}
