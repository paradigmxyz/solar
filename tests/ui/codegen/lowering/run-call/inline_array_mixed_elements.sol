//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: InlineArrayMixedElements::signed => [100, 0, -100]
//@ run-call: InlineArrayMixedElements::widenedSigned => [-1, 300]
//@ run-call: InlineArrayMixedElements::nested => [[1, -1], [-100, 0]]
//@ run-call: InlineArrayMixedElements::cleared => 0
//@ run-call: InheritanceSpecifier::values => [100, -100]
//@ run-call: ConstructorHeader::values => [100, -100]
//@ run-call: InlineArrayMixedElements::encoded => 0x0000000000000000000000000000000000000000000000000000000000000064ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff9c

// The destination's element type seeds an inline array literal, so elements that have no common
// type of their own (`uint8` literals and `int256`) each convert to `int256`. solc rejects these
// literals; see TYPECK-003 in `docs/SOLC_DIVERGENCE.md`.

contract Base {
    int256[2] stored;

    constructor(int256[2] memory a) {
        stored = a;
    }

    function values() external view returns (int256[2] memory) {
        return stored;
    }
}

contract InheritanceSpecifier is Base([100, int256(-100)]) {}

contract ConstructorHeader is Base {
    constructor() Base([100, int256(-100)]) {}
}

contract InlineArrayMixedElements {
    uint256[] s;

    // CHECK-LABEL: fn @signed(
    // CHECK: mstore v{{[0-9]+}}, 100
    // CHECK: mstore v{{[0-9]+}}+32, 0
    // CHECK: mstore v{{[0-9]+}}+64, 0xff{{f+}}9c
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
    // CHECK: mstore v{{[0-9]+}}+32, 0x{{f+}}{{$}}
    // CHECK: mstore v{{[0-9]+}}, 0xff{{f+}}9c
    function nested() external pure returns (int256[2][2] memory) {
        int256[2][2] memory a = [[1, -1], [int256(-100), 0]];
        return a;
    }

    function cleared() external returns (uint256) {
        s.push(1);
        s = [];
        return s.length;
    }

    function encoded() external pure returns (bytes memory) {
        int256[2] memory a = [100, int256(-100)];
        return abi.encode(a);
    }
}
