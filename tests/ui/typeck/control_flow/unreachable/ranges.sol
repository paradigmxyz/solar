// Unbraced bodies end where their last statement does, and the range of a `for` loop precedes its
// initializer.
contract C {
    uint x;

    function r() internal pure {
        revert();
    }

    function nestedIf(bool c, bool d) public {
        revert();
        if (c) if (d) x = 1; //~ WARN: unreachable code
    }

    function whileIfElse(bool c, bool d) public {
        revert();
        while (c) if (d) x = 1; else x = 2; //~ WARN: unreachable code
    }

    function forBody() public {
        revert();
        for (uint i; i < 2; i++) x = i; //~ WARN: unreachable code
    }

    function forWhile(bool c) public {
        revert();
        for (x = 1; c; ) while (c) x--; //~ WARN: unreachable code
    }

    function elseIf(bool c) public {
        revert();
        if (c) { x = 1; } else if (!c) x = 2; //~ WARN: unreachable code
    }

    function revertingInit(bool c) public {
        for (r(); c; ) if (c) x = 1;
        //~^ WARN: unreachable code
        //~| WARN: unreachable code
    }

    function revertingUpdate() public {
        for (uint i = 0; i < 2; r()) x = i;
    }

    function doWhile(bool c) public {
        revert();
        do x++; while (c); //~ WARN: unreachable code
    }

    function uncheckedFor() public {
        revert();
        unchecked { for (uint i; i < 2; i++) x++; } //~ WARN: unreachable code
    }

    function ifFor(bool c) public {
        revert();
        if (c) for (uint i; i < 2; i++) x++; //~ WARN: unreachable code
    }

    function ifAssembly(bool c) public {
        revert();
        if (c) assembly { sstore(0, 1) } //~ WARN: unreachable code
    }
}
