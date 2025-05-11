contract A {
    struct S0 {
        address a;
    }
    struct S1 {
        uint256 a;
        S0 s;
    }
    struct S2 {
        uint256 b;
        S0 s;
    }

    S2 public b;

    function f(S1 memory a) external {}
    //~^ ERROR: function overload clash during conversion to external types for arguments

    function f(S2 memory a) external {}
    //~^ HELP: other declaration is here
}

contract C {
    enum a {
        X
    }

    function f(a) public {}
    //~^ HELP: other declaration is here
}

contract D is C {
    function f(uint8 a) public {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
}

contract E {
    function f(address) external pure {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
    
    function f(address payable) external pure {}
    //~^ HELP: other declaration is here

    function c(address) public pure {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
    
    function c(address payable) public pure {}
    //~^ HELP: other declaration is here
}

type MyAddress is address;

interface I {}

contract F {
    function f(MyAddress a) external {}
    //~^ ERROR: function overload clash during conversion to external types for arguments

    function f(address a) external {}
    //~^ HELP: other declaration is here
}

contract G {
    function g(MyAddress a) external {}
    //~^ HELP: other declaration is here
}

contract H is G {
    function g(I a) external {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
}
