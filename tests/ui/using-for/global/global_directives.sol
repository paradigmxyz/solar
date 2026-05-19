// Solc test: test/libsolidity/syntaxTests/using/global_working.sol.

//@compile-flags: -Ztypeck

import {S, U} from "./auxiliary/global_directives.sol";

contract C {
    function f(S memory s, U u) public pure returns (uint256) {
        return s.sValue() + u.unwrap();
    }
}
