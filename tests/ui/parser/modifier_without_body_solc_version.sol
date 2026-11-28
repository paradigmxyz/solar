//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

contract A {
    //~[0_8]^ ERROR: contract `A` has unimplemented functions
    modifier m();
    //~[0_8]^ ERROR: modifiers without implementation must be marked `virtual`
    //~[0_9]^^ ERROR: modifiers without implementation are not allowed
}
