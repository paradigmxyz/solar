// OK
function f1(uint) pure {}
function f1(int) pure {}

event Ev1(uint);
event Ev1(int);

// Not OK
error Er1(uint);
error Er1(int); //~ ERROR: already declared

contract C {
    // OK
    function f2(uint) public  pure {}
    function f2(int) public pure {}

    event Ev2(uint);
    event Ev2(int);

    // Not OK
    modifier m(uint) { _; }
    modifier m(int) { _; } //~ ERROR: already declared

    error Er2(uint);
    error Er2(int); //~ ERROR: already declared
}

contract C {} //~ ERROR: already declared

contract FunctionEvent {
    function dup() public {}
    event dup(); //~ ERROR: already declared
}

contract EventFunction {
    event dup();
    function dup() public {} //~ ERROR: already declared
}

contract VariableEvent {
    uint x;
    event x(); //~ ERROR: already declared
    uint public y;
    event y(); //~ ERROR: already declared
}
