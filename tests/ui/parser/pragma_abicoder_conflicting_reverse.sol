// ported-from: test/libsolidity/syntaxTests/abiEncoder/conflicting_settings_reverse.sol
pragma abicoder v1;
pragma abicoder v2; //~ ERROR: ABI coder has already been selected for this source unit
