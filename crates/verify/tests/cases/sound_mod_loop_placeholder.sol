pragma solidity ^0.8.20;
// A modifier that runs `_` inside a loop runs the body three times; the return variable keeps
// its value between runs (legacy and via-IR alike), so `result` is 3. The function's locals are
// out of the loop head's reach, so such a modifier is refused.
contract ModLoopPlaceholder {
    modifier loop3() {
        for (uint256 i; i < 3; i++) {
            _;
        }
    }

    /// @custom:ensures result == 1
    // expect: modifier placeholder `_` inside a loop is not supported
    function bad_loop() external pure loop3 returns (uint256 result) {
        result += 1;
    }

    /// @custom:ensures result == 3
    // expect: modifier placeholder `_` inside a loop is not supported
    function bad_loop_true_spec() external pure loop3 returns (uint256 result) {
        result += 1;
    }
}
