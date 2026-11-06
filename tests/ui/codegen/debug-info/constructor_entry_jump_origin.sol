//@ compile-flags: -Onone --emit=srcmap -Zevm-ir-pipeline=none -Zdump=evm-ir
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The jump over the constructor's helpers enters the constructor, so it maps to the constructor's
// declaration, as solc maps the code `appendConstructor` emits in the location setter of the
// constructor. The constructor spans 134..187; `helper` spans 40..128.

// CHECK-LABEL: @module C_deployment
// CHECK: mstore !metadata(span=[[#CTOR_START:]]..[[#CTOR_END:]])
// CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[#CTOR_START]]..[[#CTOR_END]])
// CHECK-NEXT: bb{{[0-9]+}} [invoke=helper@
contract C {
    uint256 public x;

    function helper(uint256 a) internal pure returns (uint256) {
        return a + 1;
    }

    constructor(uint256 a) {
        x = helper(a);
    }
}
