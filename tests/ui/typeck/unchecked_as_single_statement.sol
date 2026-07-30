contract C {
    function f() public pure {
        while (true) unchecked {} //~ ERROR: `unchecked` blocks can only be used inside regular blocks

        do unchecked {} while (true); //~ ERROR: `unchecked` blocks can only be used inside regular blocks

        for (;;) unchecked {} //~ ERROR: `unchecked` blocks can only be used inside regular blocks

        if (true) unchecked {} //~ ERROR: `unchecked` blocks can only be used inside regular blocks

        if (true) {} else unchecked {} //~ ERROR: `unchecked` blocks can only be used inside regular blocks

        // Nested inside a regular block is fine.
        while (true) {
            unchecked {}
        }
        if (true) {
            unchecked {}
        } else {
            unchecked {}
        }
    }
}
