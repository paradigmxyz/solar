// ported-from: test/libsolidity/syntaxTests/abiEncoder/conflicting_settings.sol
pragma abicoder               v2;
pragma abicoder v1; //~ ERROR: ABI coder has already been selected for this source unit
