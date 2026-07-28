//@ run-call: BaseConstructorArgs::value(); constructor=[5] => 11
//@ run-call: BaseConstructorArgs::labelHash(); constructor=[5] => 0x14502d3ab34ae28d404da8f6ec0501c6f295f66caa41e122cfa9b1291bc0f9e8

contract Root {
    uint256 public value;
    string internal label;

    constructor(uint256 value_, string memory label_) {
        value = value_;
        label = label_;
    }
}

contract Middle is Root {
    constructor(uint256 value_, string memory label_) Root(value_ + 1, label_) {}
}

contract BaseConstructorArgs is Middle {
    constructor(uint256 value_) Middle(value_ * 2, "ok") {}

    function labelHash() external view returns (bytes32) {
        return keccak256(bytes(label));
    }
}
