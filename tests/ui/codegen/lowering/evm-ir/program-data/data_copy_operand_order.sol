//@ compile-flags: -O none -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

contract Child {
    uint256 public x;
}

// A data copy places its destination on the stack before the relocation push,
// then swaps them. Pushing the relocation first would bury the destination one
// slot deeper, out of `DUP16` reach when it already sits fifteen deep.
// CHECK-LABEL: (runtime) ===
// CHECK: push_data Child_creation_code_0
// CHECK-NEXT: swap 1
// CHECK-NEXT: codecopy
contract DataCopyOperandOrder {
    function f() external pure returns (uint256) {
        return type(Child).creationCode.length;
    }
}
