//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

abstract contract A {
    modifier m() virtual { _; }
    //~[0_9]^ ERROR: modifiers cannot be marked `virtual` or `override`
}

contract B is A {
    modifier m() override { _; }
    //~[0_9]^ ERROR: modifiers cannot be marked `virtual` or `override`
    //~[0_9]| ERROR: modifier `m` is already defined in a base contract
}
