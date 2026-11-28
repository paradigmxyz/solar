//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

contract A {
    modifier m() { _; }
    //~[0_8]^ ERROR: cannot override non-virtual modifier
}

contract B is A {
    modifier m() { _; }
    //~[0_8]^ ERROR: overriding modifier is missing `override` specifier
    //~[0_9]^^ ERROR: modifier `m` is already defined in a base contract
}
