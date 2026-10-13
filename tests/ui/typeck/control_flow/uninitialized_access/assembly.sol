// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/assembly.sol
contract C {
	uint[] r;
    function f() internal view returns (uint[] storage s) {
        assembly { pop(s.slot) } //~ ERROR: this variable is of storage pointer type and can be accessed
        s = r;
    }
}
