//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/using/imported_functions.sol
// ported-from: test/libsolidity/syntaxTests/using/global_local_clash.sol

import {S, f1 as f, gen, inc as aliasedInc} from "./auxiliary/imported_aliases_and_clashes.sol";
import "./auxiliary/imported_aliases_and_clashes.sol" as Imports;

using {Imports.inc, aliasedInc} for uint256;

contract ImportedFunctions {
    function f(uint256 x) public pure returns (uint256) {
        return x.inc() + x.aliasedInc();
    }
}

contract AttachedMemberClash {
    using {f} for S;

    function test() public pure returns (uint256) {
        return gen().f(); //~ ERROR: member `f` not unique
    }
}
