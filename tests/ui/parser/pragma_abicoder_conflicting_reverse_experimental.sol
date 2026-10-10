// ported-from: test/libsolidity/syntaxTests/abiEncoder/conflicting_settings_reverse_experimental.sol
pragma abicoder v1;
pragma experimental ABIEncoderV2; //~ ERROR: ABI coder v1 has already been selected through `pragma abicoder v1`
