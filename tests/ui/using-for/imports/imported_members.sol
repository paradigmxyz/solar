//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/module_2.sol
// ported-from: test/libsolidity/syntaxTests/using/library_import_as.sol

import {inc as Id, Lib as M} from "./auxiliary/imported_using.sol";

using {Id} for uint256;
using M for uint256;

contract C {
    function f(uint256 x) public pure returns (uint256) {
        return x.Id() + x.twice();
    }
}
