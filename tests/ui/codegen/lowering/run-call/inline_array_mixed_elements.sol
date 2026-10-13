//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: signed => [100, 0, -100]
//@ run-call: widenedSigned => [-1, 300]
//@ run-call: nested => [[1, -1], [-100, 0]]
//@ run-call: encoded => 0x0000000000000000000000000000000000000000000000000000000000000064ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9c

// The destination's element type seeds an inline array literal, so elements that have no common
// type of their own (`uint8` literals and `int256`) each convert to `int256`. solc rejects these
// literals; see TYPECK-003 in `docs/SOLC_DIVERGENCE.md`.

contract InlineArrayMixedElements {
    // CHECK-LABEL: fn @signed(
    // CHECK: memory_object_store_element {{.*}}, 0, 100
    // CHECK: memory_object_store_element {{.*}}, 1, 0
    // CHECK: memory_object_store_element {{.*}}, 2, 0xff{{f+}}9c
    function signed() external pure returns (int256[3] memory) {
        return [100, 0, int256(-100)];
    }

    // CHECK-LABEL: fn @widenedSigned(
    // CHECK: sext i8 255 to i16
    function widenedSigned() external pure returns (int16[2] memory) {
        int16[2] memory a = [int8(-1), 300];
        return a;
    }

    // CHECK-LABEL: fn @nested(
    // CHECK: memory_object_store_element {{.*}}, 1, 0xff{{f+}}
    // CHECK: memory_object_store_element {{.*}}, 0, 0xff{{f+}}9c
    function nested() external pure returns (int256[2][2] memory) {
        int256[2][2] memory a = [[1, -1], [int256(-100), 0]];
        return a;
    }

    function encoded() external pure returns (bytes memory) {
        int256[2] memory a = [100, int256(-100)];
        return abi.encode(a);
    }
}
