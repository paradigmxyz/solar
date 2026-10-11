//@ revisions: strict recover
//@[recover] compile-flags: -Zrecover-incomplete-input

function f() pure {
    uint i;
    do ++i; while (false);
    do i += 1; while (true && false);
    do if (true) i++; while (false);
    do while (false) i++; while (false);
    do for (; false;) i++; while (false);
    do do i++; while (false); while (false);
    do { i++; } while (false);
}
