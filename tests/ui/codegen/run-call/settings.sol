//@ run-call: configured 2; constructor=[40, true], gas=100000, value=3 => 45, true
//@ run-call: DynamicConstructorBounds::result; constructor=[0x010203, [1, 2, 3]] => 6

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

contract DynamicConstructorBounds {
    uint256 public immutable result;

    constructor(bytes memory data, uint256[] memory values) {
        result = data.length + values.length;
    }
}
