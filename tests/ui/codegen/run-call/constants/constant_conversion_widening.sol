//@ codegen-matrix: standard
//@ run-call: widened => -90, -90, 160

// A constant converts its value to the declared type, so a narrower signed value is
// sign-extended. The values come from solc.
interface I {
    function f() external;
}

contract ConstantConversionWidening {
    int8 constant C = int8(int256(uint256(keccak256(abi.encodeCall(I.f, ()))) % 100)) - 100;
    int16 constant D = C;
    int256 constant E = D;
    uint8 constant U = uint8(uint256(keccak256(abi.encodeCall(I.f, ()))) % 100) + 150;
    uint16 constant U16 = U;

    function widened() external pure returns (int16, int256, uint16) {
        return (D, E, U16);
    }
}
