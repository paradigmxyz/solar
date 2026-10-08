// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/unreachable.sol
contract C {
    uint[] s;
    function f() internal returns (uint[] storage a)
    {
        revert();
        a[0] = 0; //~ WARN: unreachable code
        a = s;
    }
}
