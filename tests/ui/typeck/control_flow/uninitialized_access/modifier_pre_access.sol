// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/modifier_pre_access.sol
contract C {
    uint[] s;
    modifier mod(uint[] storage b) {
        b[0] = 0;
        _;
    }
    function f() mod(a) internal returns (uint[] storage a) //~ ERROR: this variable is of storage pointer type and can be accessed
    {
		a = s;
    }
}
