// ported-from: test/libsolidity/syntaxTests/abiEncoder/same_setting_twice.sol
pragma experimental ABIEncoderV2;
pragma abicoder v2; //~ ERROR: ABI coder has already been selected for this source unit
