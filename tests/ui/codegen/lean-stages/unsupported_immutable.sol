//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// An address immutable is pushed from a 20-byte placeholder, of which the deployment code writes
// only the low bytes, which the Lean semantics cannot represent, so exporting fails closed.

contract LeanStagesImmutable {
    address immutable owner;

    constructor(address a) {
        owner = a;
    }

    function f() external view returns (address) {
        return owner;
    }
}

//~? ERROR: -Zemit-lean-stages: scheduled runtime EVM IR of `LeanStagesImmutable`: bb4 (block 4): an immutable placeholder of 20 bytes has no Lean semantics
