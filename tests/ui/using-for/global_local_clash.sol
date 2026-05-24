//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/global_local_clash.sol

import {S, f1 as f, gen} from "./auxiliary/imported_aliases_and_clashes.sol";

contract AttachedMemberClash {
    using {f} for S;

    function test() public pure returns (uint256) {
        return gen().f(); //~ ERROR: member `f` not unique
    }
}
