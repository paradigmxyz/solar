// ported-from: test/libsolidity/syntaxTests/pragma/version_component_above_signed_int_greater.sol
// ported-from: test/libsolidity/syntaxTests/pragma/version_component_above_signed_int_less.sol
// ported-from: test/libsolidity/syntaxTests/pragma/version_component_signed_int_min_greater.sol
// ported-from: test/libsolidity/syntaxTests/pragma/version_component_signed_int_min_less.sol

// Version components of 2^31 and above must not wrap around when compared. Mismatches are warnings.
pragma solidity >3000000000.0.0; //~ WARN: source file requires different compiler version
pragma solidity <3000000000.0.0;
pragma solidity >2147483648.0.0; //~ WARN: source file requires different compiler version
pragma solidity <2147483648.0.0;
