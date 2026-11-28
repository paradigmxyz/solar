// ported-from: test/libsolidity/syntaxTests/controlFlow/modifiers/modifier_override.sol
contract A {
	function f() mod internal returns (uint[] storage) { //~ ERROR: this variable is of storage pointer type and can be returned
	//~^ WARN: unreachable code
	}
	modifier mod() virtual {
		revert();
		_;
	}
}
contract B is A {
	modifier mod() override { _; }
	function g() public {
		f()[0] = 42;
	}
}
