// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/always_revert.sol
contract C {
    function f() internal view returns(uint[] storage a)
    {
        uint b = a[0];
        revert();
        b;
    }
}
