// ported-from: test/libsolidity/syntaxTests/array/length/abi_decode_length_too_large.sol

// Used to cause ICE
contract C {
	function f() public {
		abi.decode("", (bytes1[999999999])); //~ ERROR: type too large for memory
	}
}
