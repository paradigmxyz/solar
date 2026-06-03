//@ compile-flags: -Ztypeck

// An abstract contract may inherit `is Base` without supplying the base
// constructor's arguments; a concrete descendant supplies them. This must not
// be reported as a wrong-argument-count error.
abstract contract Base {
    constructor(uint256 x) {
        require(x != 0);
    }
}

abstract contract Mid is Base {}

contract Final is Mid {
    constructor() Base(1) {}
}
