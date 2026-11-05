contract A { //~ ERROR: contract `A` has unimplemented functions
    function f() public; //~ ERROR: functions without implementation must be marked virtual
}

contract B {
    function f() private virtual {} //~ ERROR: `virtual` and `private` cannot be used together
}

library L { //~ ERROR: contract `L` has unimplemented functions
    function f() public; //~ ERROR: library functions must be implemented if declared
}

// A modifier is not a function in solc, so an unimplemented one gets its own error.
contract M { //~ ERROR: contract `M` has unimplemented functions
    modifier m(); //~ ERROR: modifiers without implementation must be marked `virtual`
}

// Valid cases
abstract contract ValidAbstract {
    function f() public virtual;
    modifier m() virtual;
}

contract ValidConcrete {
    function f() public {}
}

library ValidLib {
    function f() public {}
}
