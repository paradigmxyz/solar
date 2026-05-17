//@compile-flags: -Ztypeck

import "./auxiliary/file_level_using.sol";

function f(uint256 x) pure returns (uint256) {
    return x.id(); //~ ERROR: member `id` not found
}
