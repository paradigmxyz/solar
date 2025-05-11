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

    function f(S2 memory a) external {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
}

contract C {
    enum a {
        X
    }

    function f(a) public {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
}

contract D is C {
    function f(uint8 a) public {}
}

contract E {
    function f(address) external pure {}

    function f(address payable) external pure {}
    //~^ ERROR: function overload clash during conversion to external types for arguments

    function c(address) public pure {}

    function c(address payable) public pure {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
}

type MyAddress is address;

interface I {}

contract F {
    function f(MyAddress a) external {}

    function f(address a) external {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
}

contract G {
    function g(MyAddress a) external {}
    //~^ ERROR: function overload clash during conversion to external types for arguments
}

contract H is G {
    function g(I a) external {}
}
