// ported-from: test/libsolidity/syntaxTests/constructor/nonabiv2_type.sol
pragma abicoder v1;
contract C {
	constructor(uint[][][] memory t) {} //~ ERROR: this type is only supported in ABI coder v2
}
