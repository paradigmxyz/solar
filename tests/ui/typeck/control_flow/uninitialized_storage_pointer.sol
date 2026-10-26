struct Item {
    uint x;
}

contract C {
    function f(uint x) public returns (uint) {
        Item storage y; //~ NOTE: the variable was declared here
        y.x = x; //~ ERROR: this variable is of storage pointer type and can be accessed
        return y.x; //~ ERROR: this variable is of storage pointer type and can be accessed
    }

    function selfAssign() internal pure {
        Item storage y;
        y = y;
    }
}

// Each base function is checked in every derived contract but reported once.
contract A {
    uint[] s;

    function f() internal returns (uint[] storage r) { //~ NOTE: the variable was declared here
        r.push(); //~ ERROR: this variable is of storage pointer type and can be accessed
        r = s;
    }
}

contract B is A {}

contract D is B {}
