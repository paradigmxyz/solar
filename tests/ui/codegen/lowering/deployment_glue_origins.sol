//@ compile-flags: -Ogas --emit=srcmap -Zdump=evm-ir
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The code that copies the constructor arguments belongs to the constructor, as in solc's
// `appendConstructor`, and the code that returns the runtime belongs to the contract, as in
// `packIntoContractCreator`. Neither is a MIR instruction.

// CHECK-LABEL: (creation) ===
// CHECK: codesize !metadata(span=[[CTOR:[0-9]+..[0-9]+]])
// CHECK: codecopy !metadata(span=[[CTOR]])
// CHECK: mstore !metadata(span=[[CTOR]])
// CHECK: push 0 !metadata(span=[[C:[0-9]+..[0-9]+]])
// CHECK-NEXT: codecopy !metadata(span=[[C]])
// CHECK: return !metadata(span=[[C]])
contract C {
    uint256 immutable x;

    constructor(uint256 a) {
        x = a;
    }

    function f() external view returns (uint256) {
        return x;
    }
}
