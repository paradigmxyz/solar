//@ codegen-matrix: standard
//@ run-call: x => 7
//@ run-call: y => 9

// A constructor's assembly stores to fixed addresses must not overwrite staged immutables.
contract C {
    uint256 public immutable x;
    uint256 public immutable y;
    constructor() {
        x = 7;
        y = 9;
        assembly {
            mstore(0xc0, 0x3333)
            mstore(0xe0, 0x4444)
        }
    }
}
