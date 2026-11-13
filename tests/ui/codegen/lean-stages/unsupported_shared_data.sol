//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// MIR numbers the program data of creation and runtime code together, and the runtime code
// keeps only its own, numbered again, so the data a MIR stage copies cannot be found in the
// assembled runtime code by its number, and exporting fails closed.

contract LeanStagesSharedData {
    event E(bytes data);

    constructor() {
        emit E(hex"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }

    function f() external pure returns (bytes memory) {
        return hex"111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
    }
}

//~? ERROR: -Zemit-lean-stages: in `Stage0.lean` of `LeanStagesSharedData`: the runtime code has no program data 1
