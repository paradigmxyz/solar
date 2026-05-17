//@compile-flags: -Ztypeck

import {inc, Lib} from "./auxiliary/imported_using.sol";

using {inc} for uint256;
using Lib for uint256;

contract C {
    function f(uint256 x) public pure returns (uint256) {
        return x.inc() + x.twice();
    }
}
