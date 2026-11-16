contract C {
    uint256 t;

    modifier none() {
        _;
    }

    modifier two(uint256 a, uint256 b) {
        t = a * 10 + b;
        _;
    }

    function ok1() external none {}
    function ok2() external none() {}
    function ok3() external two(1, 2) {}
    function ok4() external two({b: 2, a: 1}) {}

    function missing() external two {} //~ ERROR: wrong argument count for modifier invocation: 0 arguments given but expected 2
    function tooFew() external two(1) {} //~ ERROR: wrong argument count for modifier invocation: 1 arguments given but expected 2
    function tooMany() external two(1, 2, 3) {} //~ ERROR: wrong argument count for modifier invocation: 3 arguments given but expected 2
    function badType() external two("a", 1) {} //~ ERROR: mismatched types
    function extra() external none(1) {} //~ ERROR: wrong argument count for modifier invocation: 1 arguments given but expected 0

    function duplicateName() external two({a: 1, a: 2}) {} //~ ERROR: duplicate named argument `a`
    function unknownName() external two({a: 1, c: 2}) {} //~ ERROR: named argument `c` does not match function declaration
    function tooFewNames() external two({a: 1}) {} //~ ERROR: wrong argument count for modifier invocation: 1 arguments given but expected 2
    function badNamedType() external two({b: 1, a: "a"}) {} //~ ERROR: mismatched types
}
