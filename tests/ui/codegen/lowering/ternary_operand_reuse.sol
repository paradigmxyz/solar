//@compile-flags: --emit=bin-runtime -Zdump=evm-ir-runtime
//@filecheck:

contract TernaryOperandReuse {
    // The planner may consume `caller()` as the modulus because the return can
    // rematerialize it instead of retaining a stack copy.
    // CHECK-LABEL: {{^}}bb9:
    // CHECK: caller
    // CHECK-NOT: pop
    // CHECK: return
    function addCaller(uint256 x, uint256 y) external view returns (uint256, address) {
        assembly {
            let sender := caller()
            let result := addmod(x, y, sender)
            mstore(0, result)
            mstore(32, sender)
            return(0, 64)
        }
    }

    // Repeated ternary operands need two input words; the return can rematerialize
    // its later occurrence instead of keeping a third physical copy.
    // CHECK-LABEL: {{^}}bb8:
    // CHECK: caller
    // CHECK-NEXT: caller
    // CHECK-NEXT: mulmod
    function mulRepeated(uint256 modulus) external view returns (uint256, address) {
        assembly {
            let sender := caller()
            let result := mulmod(sender, sender, modulus)
            mstore(0, result)
            mstore(32, sender)
            return(0, 64)
        }
    }
}
