contract C {
    struct S {
        uint x;
    }

    uint[] data;
    S s;

    function f(uint[] storage p) internal returns (S storage r) {
        r = s;
        uint[] storage x = data;

        delete p; //~ ERROR: cannot delete `uint256[] storage`
        delete r; //~ ERROR: cannot delete `struct C.S storage`
        delete (x); //~ ERROR: cannot delete `uint256[] storage`

        delete data;
        delete s;
        delete x[0];
        delete r.x;
    }
}
