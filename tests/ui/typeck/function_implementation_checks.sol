contract A { //~ ERROR: contract `A` should be marked as abstract
    function f() public; //~ ERROR: functions without implementation must be marked virtual
}

contract B {
    function f() private virtual {} //~ ERROR: `virtual` and `private` cannot be used together
}

library L { //~ ERROR: contract `L` should be marked as abstract
    function f() public; //~ ERROR: library functions must be implemented if declared
}

// Valid cases
abstract contract ValidAbstract {
    function f() public virtual;
}

contract ValidConcrete {
    function f() public {}
}

library ValidLib {
    function f() public {}
}
