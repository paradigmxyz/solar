//@ compile-flags: -Ztypeck

function test() pure {
    uint[2**3**2] memory a;
    uint[512] memory b = a;

    uint[8 - 3 - 2] memory c;
    uint[3] memory d = c;
}
