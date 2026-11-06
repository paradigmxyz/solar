//@ codegen-matrix: standard
//@ run-call: x; constructor=[1, 2] => 3
//@ run-call: h; constructor=[1, 2] => 0x5ae94f111b94bd3242b5d5b03dc030cec8f519ada24b477ab2854ad3b6059211

// A constructor with arguments reads its argument blob from the end of the code, so literal data
// that `pack-data` adds must go before the runtime code, the last entry.
contract ConstructorArgsLongLiteral {
    uint256 public x;
    bytes32 public h;

    constructor(uint256 a, uint256 b) {
        require(a < 100, "this constructor argument check has a message that is longer than thirty two bytes so its data is not one word");
        x = a + b;
        h = keccak256("this constructor argument check has a message that is longer than thirty two bytes so its data is not one word");
    }
}
