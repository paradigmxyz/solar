//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/global_working.sol

import {gen as g} from "./auxiliary/global_working.sol";

function test() pure {
    uint256 p = g().f();
    p++;
}
