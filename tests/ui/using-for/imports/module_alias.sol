//@ check-pass
// ported-from: test/libsolidity/syntaxTests/using/module_3.sol

import "./auxiliary/imported_using.sol" as M;

using {M.inc} for uint256;
using M.Lib for uint256;

contract C {
    function f(uint256 x) public pure returns (uint256) {
        return x.inc() + x.twice();
    }
}
