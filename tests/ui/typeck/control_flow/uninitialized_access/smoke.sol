// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/smoke.sol
contract C {
    uint[] s;
    function f() internal returns (uint[] storage a)
    {
        a[0] = 0; //~ ERROR: this variable is of storage pointer type and can be accessed
        a = s;
    }
}
