//@ codegen-matrix: standard
//@ run-call: x => 7

// A range too large to expand memory to names no staging address, even when a branch that
// never runs names it, so the deployment does not run out of gas.
contract C {
    uint256 public immutable x;
    constructor() payable {
        if (msg.value != 0) {
            assembly { revert(0, 0x100000000) }
        }
        x = 7;
    }
}
