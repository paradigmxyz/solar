contract C {
    function f(uint u, bytes32 s, bool b) internal {}

    function call() public {
        f({s: "abc", u: 1,     b: true});
        f({s: "abc", b: true,  u: 1});
        f({u: 1,     s: "abc", b: true});
        f({b: true,  s: "abc", u: 1});
        f({u: 1,     b: true,  s: "abc"});
        f({b: true,  u: 1,     s: "abc"});
    }
}

contract Created {
    constructor(uint count, bool enabled) {}
}

contract Factory {
    function create() external returns (Created) {
        return new Created({enabled: true, count: 7});
    }
}
