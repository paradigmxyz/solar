// ported-from: test/libsolidity/semanticTests/using/using_global_all_the_types.sol

import {E, S, T} from "./auxiliary/global_library.sol";

contract C {
    function f(E e, S memory s, T t) public pure returns (uint256, uint256, uint256) {
        return (e.f(), s.f(), t.f());
    }
}
