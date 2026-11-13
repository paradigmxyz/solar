//@ run-call: double 21 => 42

import "./auxiliary/Common.sol" as Common;

contract Test {
    function double(uint256 x) external pure returns (uint256) {
        return Common.double(x);
    }
}
