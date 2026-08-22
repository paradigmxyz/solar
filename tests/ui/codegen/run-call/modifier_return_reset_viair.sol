//@ run-call: foo() => 0
// Solc's via-IR modifier frames reset return variables for each placeholder.

contract ModifierReturnResetViaIr {
    bool private active = true;

    modifier twice() {
        _;
        active = false;
        _;
    }

    function foo() external twice returns (uint256 result) {
        if (active) result = 1;
    }
}
