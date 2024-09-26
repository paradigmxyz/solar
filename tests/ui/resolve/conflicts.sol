// OK
function f1(uint) pure {}
function f1(int) pure {}

event Ev1(uint);
event Ev1(int);

// Not OK
error Er1(uint);
error Er1(int); //~ ERROR already declared

contract C {
    // OK
    function f2(uint) pure {}
    function f2(int) pure {}

    event Ev2(uint);
    event Ev2(int);

    // Not OK
    modifier m(uint) { _; }
    modifier m(int) { _; } //~ ERROR already declared

    error Er2(uint);
    error Er2(int); //~ ERROR already declared
}

contract C {} //~ ERROR already declared
