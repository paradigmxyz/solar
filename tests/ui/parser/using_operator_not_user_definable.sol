// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_parsing_non_user_definable.sol

// The parser rejects the first non-user-definable operator and does not recover
// within the directive, so later upstream variants are unreachable here.
using {f as new} for uint256; //~ ERROR: expected
