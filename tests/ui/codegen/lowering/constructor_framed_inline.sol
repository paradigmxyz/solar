//@ compile-flags: -Ogas
//@ run-call: value => 42

contract ConstructorFramedInline {
    uint256 public value;

    constructor() {
        value = make(41);
        bytes memory data = new bytes(32);
        data[0] = bytes1(uint8(1));
        value += uint8(data[0]);
    }

    function make(uint256 word) internal pure returns (uint256 result) {
        assembly {
            result := word
        }
    }
}
