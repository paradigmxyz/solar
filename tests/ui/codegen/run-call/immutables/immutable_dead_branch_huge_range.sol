//@ codegen-matrix: standard
//@ run-call: x => 7
//@ run-call: y => 9

// A branch that never runs must not move the staging words by naming a large range: a revert
// reads only revert data, and a range too large to expand memory to runs out of gas anyway.
contract C {
    uint256 public immutable x;
    uint256 public immutable y;
    constructor() payable {
        if (msg.value != 0) {
            assembly { revert(0, 0x100000) }
        }
        if (msg.value > 1) {
            assembly { calldatacopy(0, 0, 0x100000000) }
        }
        x = 7;
        y = 9;
    }
}
