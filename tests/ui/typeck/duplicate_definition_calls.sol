// Calls and references that pick among duplicate definitions only report the duplicates.

library L {
    function f(uint) internal {} //~ ERROR: function with same name and parameter types declared twice
    function f(uint) internal {}
}

contract A {
    event Inherited(uint);
}

contract B is A {
    event Inherited(uint indexed); //~ ERROR: event with same name and parameter types declared twice

    event Own(uint); //~ ERROR: event with same name and parameter types declared twice
    event Own(uint) anonymous;

    function f(uint) public {} //~ ERROR: function with same name and parameter types declared twice
    function f(uint) public {}

    function g() public {
        emit Inherited(1);
        emit B.Inherited(1);
        emit Own(1);
        f(1);
        this.f(1);
        L.f(1);
        bytes32 selector = Inherited.selector;
        selector = B.Own.selector;
        function(uint) internal internalRef = f;
        function(uint) external externalRef = this.f;
    }
}

contract C {
    string s;

    function f(string storage) internal {}
    function f(string memory) public {}

    function g() public {
        f(s); //~ ERROR: no unique declarations found
    }
}
