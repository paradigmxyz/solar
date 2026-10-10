// ported-from: test/libsolidity/syntaxTests/controlFlow/modifiers/modifier_different_functions.sol
contract A {
	function f() mod internal returns (uint[] storage) {
		revert();
	}
	function g() mod internal returns (uint[] storage) { //~ ERROR: this variable is of storage pointer type and can be returned
	}
	modifier mod() virtual {
		_;
	}
}
