//@compile-flags: -Ztypeck

import {E, S, T} from "./auxiliary/global_library.sol";

contract C {
    function f(E e, S memory s, T t) public pure returns (uint256, uint256, uint256, T) {
        return (e.f(), s.f(), t.f(), t.inc());
    }
}
