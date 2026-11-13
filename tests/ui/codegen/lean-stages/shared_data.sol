//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// MIR numbers the program data of creation and runtime code together, and the runtime code
// keeps only its own, numbered again, so the stages name the data a MIR stage copies by what it
// holds rather than by its number, and the export finds it in the assembled runtime code.

contract LeanStagesSharedData {
    event E(bytes data);

    constructor() {
        emit E(hex"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }

    function f() external pure returns (bytes memory) {
        return hex"111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
    }
}
