//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

contract C {
    uint count;
    uint other;

    function plain() external {
        uint count = 1 + * 2; //~[strict,recover] ERROR: expected one of
        count = 3;
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function tuple() external {
        (uint count, , uint other) = (1, * 2, 3); //~[recover] ERROR: expected one of
        //~[recover]^ ERROR: mismatched number of components
        count = other;
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function named() external {
        uint count = identity({value: 1 + * 2}); //~[recover] ERROR: expected one of
        count = 3;
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function trailing() external {
        uint count = 1 unexpected; //~[recover] ERROR: expected one of
        count = 3;
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }

    function empty() external {
        uint count = ; //~[recover] ERROR: expected one of
        count = 3;
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }
}
