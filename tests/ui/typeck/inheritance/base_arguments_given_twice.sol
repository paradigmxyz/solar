// A base constructor's arguments may only be given once in a hierarchy:
// which list the constructor would be called with is otherwise arbitrary.
contract A {
    uint256 public sa;

    constructor(uint256 a) {
        sa = a;
    }
}

abstract contract Bb is A {
    constructor(uint256 b) A(b + 10) {}
}

abstract contract Cc is A {
    constructor(uint256 c) A(c + 20) {}
}

contract TwoAncestors is Bb, Cc { //~ ERROR: base constructor arguments given twice
    constructor() Bb(1) Cc(2) {}
}

// The most derived contract can give the arguments itself, as long as no
// ancestor also gives them.
abstract contract Dd is A {
    constructor(uint256 d) {
        sa = d;
    }
}

contract OwnAndAncestor is Bb, Dd {
    constructor() A(3) Bb(1) Dd(2) {} //~ ERROR: base constructor arguments given twice
}

contract OnlyOwn is Dd {
    constructor() A(3) Dd(2) {}
}
