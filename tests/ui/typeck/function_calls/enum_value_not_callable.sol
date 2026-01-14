//@ compile-flags: -Ztypeck
enum E { A, B, C }

contract C {
    uint a = E.B(1000); //~ ERROR: expected function
}
