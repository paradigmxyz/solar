//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// `-Onone` keeps at least one instruction of its own for every statement that coverage anchors.
// The two `revert Denied()` statements keep a revert sequence each, under their own spans, and
// `hash` computes its `keccak256` at run time under the expression's span, as solc does.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: push 0xe3372e2d !metadata(span=[[FIRST:[0-9]+..[0-9]+]])
// CHECK: revert !metadata(span=[[FIRST]], exit=revert)
// CHECK: push 0xe3372e2d !metadata(span=[[SECOND:[0-9]+..[0-9]+]])
// CHECK: revert !metadata(span=[[SECOND]], exit=revert)
// CHECK: keccak256 !metadata(span=[[HASH:[0-9]+..[0-9]+]])
contract C {
    error Denied();

    function check(uint256 a, uint256 b) external pure returns (uint256) {
        if (a == 0) revert Denied();
        if (b == 0) revert Denied();
        return a + b;
    }

    function hash(uint256 a) external pure returns (bytes32) {
        return keccak256(abi.encode("a string that is longer than thirty-one bytes", a + 1, 0));
    }
}
