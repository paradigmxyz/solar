contract C {
    struct S { uint x; }
    uint y;
    function f(bool c, bool d) internal {
        if (c) {}
        S storage s;
        if (d) {}
        y = s.x; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
}
