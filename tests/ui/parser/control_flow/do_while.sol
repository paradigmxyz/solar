function f() {
    uint i;
    do ++i; while (false);
    do i += 1; while (true && false);
}
