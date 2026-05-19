// Solc tests:
// - test/libsolidity/syntaxTests/using/module_2.sol.
// - test/libsolidity/syntaxTests/using/library_import_as.sol.

//@compile-flags: -Ztypeck

import {inc, Lib} from "./auxiliary/imported_using.sol";

using {inc} for uint256;
using Lib for uint256;

contract C {
    function f(uint256 x) public pure returns (uint256) {
        return x.inc() + x.twice();
    }
}
