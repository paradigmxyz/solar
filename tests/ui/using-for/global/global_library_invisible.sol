//@compile-flags: -Ztypeck

import {Maker} from "./auxiliary/global_library.sol";

contract C {
    function f(Maker maker) public view returns (uint256) {
        return maker.make().inc().inc().dec().unwrap();
    }
}
