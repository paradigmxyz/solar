//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/using/using_global_invisible.sol

import {C} from "./auxiliary/global_invisible_mid.sol";

contract D {
    function test() public returns (uint256) {
        C c = new C();
        return c.f().inc().inc().dec().unwrap();
    }
}
