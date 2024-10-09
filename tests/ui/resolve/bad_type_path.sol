// Testing `ResolverError`.

struct S {
    uint x;
}

struct S2 {
    uint x;
}
struct S2 { //~ ERROR already declared
    uint x;
}

function f() {}
function f(uint x) {}

contract C {
    S s1;
    S.x s2; //~ ERROR `S` is a struct, which cannot be indexed in type paths

    S2 s3;
    S2.x s4; //~ ERROR `S2` is a struct, which cannot be indexed in type paths

    f f1; //~ ERROR symbol `f` resolved to multiple declarations
    f.x f2; //~ ERROR symbol `f` resolved to multiple declarations
}
