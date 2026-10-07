//@ compile-flags: --emit=srcmap-runtime --allow 2264

// Each statement adds new source locations, so a jump back to the location of
// compiler-generated code skips many entries. The debug info rows store that
// jump as a multi-byte LEB128 number.
contract C {
    uint256 x;

    function f(uint256 a) external {
        x += a * 1;
        x += a * 2;
        x += a * 3;
        x += a * 4;
        x += a * 5;
        x += a * 6;
        x += a * 7;
        x += a * 8;
        x += a * 9;
        x += a * 10;
        x += a * 11;
        x += a * 12;
        x += a * 13;
        x += a * 14;
        x += a * 15;
        x += a * 16;
        x += a * 17;
        x += a * 18;
        x += a * 19;
        x += a * 20;
        x += a * 21;
        x += a * 22;
        x += a * 23;
        x += a * 24;
        x += a * 25;
        x += a * 26;
        x += a * 27;
        x += a * 28;
        x += a * 29;
        x += a * 30;
        x += a * 31;
        x += a * 32;
        x += a * 33;
        x += a * 34;
        x += a * 35;
        x += a * 36;
        x += a * 37;
        x += a * 38;
        x += a * 39;
        x += a * 40;
    }
}
