//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

contract C {
    function f() external pure {
        uint256 i;
        do i++ while (false); //~ ERROR: expected one of
        do if (true) i++ while (false); //~[recover] ERROR: expected one of
        do while (false) i++ while (false); //~[recover] ERROR: expected one of
        do for (; false;) i++ while (false); //~[recover] ERROR: expected one of
        do do i++ while (false) while (false); //~[recover] ERROR: expected one of
        //~[recover]^ ERROR: expected `;`
        uint8 later = 300; //~[recover] ERROR: mismatched types
    }
}
