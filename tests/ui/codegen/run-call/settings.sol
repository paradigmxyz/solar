//@ run-call: configured 2; constructor=[40, true], gas=100000, value=3 => 45, true

contract RunCallSettings {
    uint256 private base;
    bool private flag;

    constructor(uint256 base_, bool flag_) {
        base = base_;
        flag = flag_;
    }

    function configured(uint256 x) external payable returns (uint256, bool) {
        require(gasleft() < 200_000);
        return (base + x + msg.value, flag);
    }
}
