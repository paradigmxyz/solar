//@ compile-flags: -Ogas --emit=ethdebug --pretty-json -Zdump=evm-ir
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The constructor loop folds four stores of literal words into one `CODECOPY`, which does the work
// of every store, so ETHDebug lists their four origins as `gather`, not as alternatives in `pick`.
// CHECK: "mnemonic": "CODECOPY"
// CHECK-NEXT: },
// CHECK-NEXT: "context": {
// CHECK-NEXT: "gather": [
// CHECK-NOT: "pick"
contract C {
    bytes32 public result;

    constructor() {
        bytes32 acc;
        for (uint256 i; i < 40; ++i) {
            assembly {
                mstore(0x00, 0x1111111111111111111111111111111111111111111111111111111111111111)
                mstore(0x20, 0x2222222222222222222222222222222222222222222222222222222222222222)
                mstore(0x40, 0x3333333333333333333333333333333333333333333333333333333333333333)
                mstore(0x60, 0x4444444444444444444444444444444444444444444444444444444444444444)
                acc := xor(acc, keccak256(0x00, 0x80))
            }
        }
        result = acc;
    }
}
