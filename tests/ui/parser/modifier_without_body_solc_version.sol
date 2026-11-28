//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

abstract contract A {
    modifier m();
    //~[0_8]^ ERROR: modifiers without implementation must be marked `virtual`
    //~[0_9]^^ ERROR: modifiers without implementation are not allowed
}
