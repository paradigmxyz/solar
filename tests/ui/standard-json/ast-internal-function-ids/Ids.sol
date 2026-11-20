contract Pointers {
    function used() internal pure {}

    function unused() internal pure {}

    function other() internal pure {}

    function run() public pure {
        function() internal pure f = other;
        f = used;
        f();
        unused();
    }
}

contract Plain {
    function called() internal pure {}

    function run() public pure {
        called();
    }
}

contract Base {
    function baseFunction() internal pure {}

    function pick() internal pure returns (function() internal pure) {
        return baseFunction;
    }
}

// The functions of a base contract are numbered with those of the derived one.
contract Derived is Base {
    function ownFunction() internal pure {}

    function run() public pure {
        function() internal pure f = ownFunction;
        f();
        pick()();
    }
}

// A function that a call names in parentheses is a function value, and one it calls directly is
// not.
contract Parenthesized {
    function direct() internal pure {}

    function parenthesized() internal pure {}

    function run() public pure {
        direct();
        (parenthesized)();
    }
}
