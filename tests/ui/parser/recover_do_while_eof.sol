//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

contract C {
    function f() external pure {
        uint256 i;
        do i++ //~ ERROR: expected one of
        //~[recover]^ ERROR: expected `while`
        //~[recover]| ERROR: expected contract item
