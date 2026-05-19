//@compile-flags: -Ztypeck

import {C} from "./auxiliary/global_invisible_mid.sol";

contract D {
    function test() public returns (uint256) {
        C c = new C();
        return c.f().inc().inc().dec().unwrap();
    }
}
