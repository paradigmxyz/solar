// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/struct.sol
contract C {
    struct S { uint a; }
    S s;
    function f() internal returns (S storage r)
    {
        r.a = 0; //~ ERROR: this variable is of storage pointer type and can be accessed
        r = s;
    }
}
