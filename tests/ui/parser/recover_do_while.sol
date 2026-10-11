//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

contract C {
    function simple() external pure {
        uint256 i;
        do i++
    } //~ ERROR: expected one of
    //~[recover]^ ERROR: expected `while`

    function nestedIf() external pure {
        uint256 i;
        do if (true) i++
    } //~[recover] ERROR: expected one of
    //~[recover]^ ERROR: expected one of `else` or `while`

    function nestedWhile() external pure {
        uint256 i;
        do while (true) i++
    } //~[recover] ERROR: expected one of
    //~[recover]^ ERROR: expected `while`

    function nestedFor() external pure {
        uint256 i;
        do for (;;) i++
    } //~[recover] ERROR: expected one of
    //~[recover]^ ERROR: expected `while`

    function nestedDo() external pure {
        uint256 i;
        do do i++
    } //~[recover] ERROR: expected one of
    //~[recover]^ ERROR: expected `while`

    function declaration() external pure {
        uint256 i;
        do i = 1
        uint8 y; //~[recover] ERROR: expected one of
        //~[recover]^ ERROR: expected `while`
    }
}
