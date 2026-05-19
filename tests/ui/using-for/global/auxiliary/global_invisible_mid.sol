// Ported from test/libsolidity/semanticTests/using/using_global_invisible.sol.

import {T} from "./global_invisible_base.sol";

contract C {
    function f() public pure returns (T r) {
        r = r.inc().inc();
    }
}
