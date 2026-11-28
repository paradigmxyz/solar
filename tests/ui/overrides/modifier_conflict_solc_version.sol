//@ revisions: 0_8 0_9
//@[0_9] compile-flags: --solc-version 0.9.0

contract A {
    modifier m() { _; }
}

contract B {
    modifier m() { _; }
}

contract C is A, B {}
//~[0_8]^ ERROR: derived contract must override modifier `m`
//~[0_9]^^ ERROR: two or more base classes define modifier `m`
