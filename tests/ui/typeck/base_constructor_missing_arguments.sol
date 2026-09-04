//@ revisions: sema bin
//@[bin] compile-flags: --emit=bin
//~[bin]? WARN: code generation is experimental
// A deployable contract that gives a parameterized base constructor no
// arguments would deploy with the base constructor, and the state-variable
// initializers it runs, skipped. The `bin` revision keeps codegen running on
// the rejected source: lowering must not emit bytecode for it and must add no
// diagnostics of its own.
contract Parameterized {
    uint256 public x = 7;

    constructor(uint256 v) {
        x = v;
    }
}

// Deferred to a derived contract.
abstract contract Abstract is Parameterized {}

contract Direct is Parameterized {} //~ ERROR: no arguments passed to the base constructor

contract WithConstructor is Parameterized { //~ ERROR: no arguments passed to the base constructor
    constructor() {}
}

contract ThroughAbstract is Abstract {} //~ ERROR: no arguments passed to the base constructor

contract Given is Parameterized(1) {}

contract GivenInHeader is Parameterized {
    constructor() Parameterized(2) {}
}

contract GivenThroughAbstract is Abstract {
    constructor() Parameterized(3) {}
}

contract Parameterless {
    uint256 public y = 9;

    constructor() {}
}

contract NoArgumentsNeeded is Parameterless {}
