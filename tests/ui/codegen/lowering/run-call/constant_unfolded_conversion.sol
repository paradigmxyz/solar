//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: ternary => -5
//@ run-call: widened => -90, -90, 160

// A constant that does not fold lowers its initializer and converts it to the declared type, so
// a narrower signed value is sign-extended. The values come from solc.
interface I {
    function f() external;
}

contract ConstantUnfoldedConversion {
    int8 constant A = -5;
    // The untaken branch overflows, so the conditional does not fold.
    int16 constant B = true ? A : A - 124;

    int8 constant C = int8(int256(uint256(keccak256(abi.encodeCall(I.f, ()))) % 100)) - 100;
    int16 constant D = C;
    int256 constant E = D;
    uint8 constant U = uint8(uint256(keccak256(abi.encodeCall(I.f, ()))) % 100) + 150;
    uint16 constant U16 = U;

    // CHECK-LABEL: fn @ternary()
    // CHECK: sext i8 {{v[0-9]+}} to i16
    function ternary() external pure returns (int16) {
        return B;
    }

    function widened() external pure returns (int16, int256, uint16) {
        return (D, E, U16);
    }
}
