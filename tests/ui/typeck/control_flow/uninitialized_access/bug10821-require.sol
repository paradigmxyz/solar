// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/bug10821-require.sol
contract Test {
	function testFunc() external {
		require(true);
		bytes storage b;
		b[0] = 0x42; //~ ERROR: this variable is of storage pointer type and can be accessed
	}
}
