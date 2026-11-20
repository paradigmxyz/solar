//@compile-flags: -Zrecover-incomplete-input

// A stray `)` in a nested block must be skipped, not re-parsed by the enclosing block.

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
}
