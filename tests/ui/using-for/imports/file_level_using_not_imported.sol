// Solc test: test/libsolidity/syntaxTests/using/file_level_inactive_after_import.sol.

//@compile-flags: -Ztypeck

import "./auxiliary/file_level_using.sol";

function f(uint256 x) pure returns (uint256) {
    return x.id(); //~ ERROR: member `id` not found
}
