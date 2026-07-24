//@ check-pass
// ported-from: test/libsolidity/semanticTests/using/imported_functions.sol

import {inc as aliasedInc} from "./auxiliary/imported_aliases_and_clashes.sol";
import "./auxiliary/imported_aliases_and_clashes.sol" as Imports;

using {Imports.inc, aliasedInc} for uint256;

contract ImportedFunctions {
    function f(uint256 x) public pure returns (uint256) {
        return x.inc() + x.aliasedInc();
    }
}
