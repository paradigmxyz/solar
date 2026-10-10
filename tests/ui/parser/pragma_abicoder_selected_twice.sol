// ported-from: test/libsolidity/syntaxTests/abiEncoder/selected_twice.sol
pragma abicoder v1;
pragma abicoder v1; //~ ERROR: ABI coder has already been selected for this source unit
