//@compile-flags: -Zrecover-incomplete-input

// A stray `)` or `]` in a nested block must be skipped, not re-parsed by the enclosing block.

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

    function orphanElse() public {
        if () {} else {} //~ ERROR: expected one of
        //~^ ERROR: expected one of
    }

    function later() public {
        uint8 value = 300; //~ ERROR: mismatched types
    }

    function bracket() public {
        while (true) {
            if (] //~ ERROR: expected one of
        }
    }
}
