// Error 5424: functions without implementation must be marked virtual
contract A {
    function f() public; //~ ERROR: functions without implementation must be marked virtual
}

// Error 3942: virtual and private cannot be used together
contract B {
    function f() private virtual {} //~ ERROR: "virtual" and "private" cannot be used together
}

// Error 9231: library functions must be implemented if declared
library L {
    function f() public; //~ ERROR: library functions must be implemented if declared
}

// Valid cases
contract ValidAbstract {
    function f() public virtual;
}

contract ValidConcrete {
    function f() public {}
}

library ValidLib {
    function f() public {}
}
