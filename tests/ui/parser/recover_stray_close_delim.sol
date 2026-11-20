//@compile-flags: -Zrecover-incomplete-input

// A stray `)` left by an incomplete `if` inside a nested block must not make the
// enclosing block parse a statement at the same token again (it used to panic).

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
