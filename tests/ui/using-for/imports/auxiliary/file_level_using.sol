// Ported from test/libsolidity/syntaxTests/using/file_level_inactive_after_import.sol.

function id(uint256 x) pure returns (uint256) {
    return x;
}

using {id} for uint256;

function local(uint256 x) pure returns (uint256) {
    return x.id();
}
