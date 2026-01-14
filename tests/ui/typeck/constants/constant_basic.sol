//@compile-flags: -Ztypeck
uint constant x = 7;

contract C {
    uint constant y = x + 1;
    uint constant z = y * 2;
}
