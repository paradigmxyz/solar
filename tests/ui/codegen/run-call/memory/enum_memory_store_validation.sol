//@ codegen-matrix: standard
//@ run-call-fail: structConstructor => Panic(0x21)
//@ run-call-fail: arrayLiteral => Panic(0x21)
//@ run-call-fail: arrayElement => Panic(0x21)

contract EnumMemoryStoreValidation {
    enum Mode {
        Zero,
        One,
        Two
    }

    struct Value {
        Mode mode;
    }

    function dirty() internal pure returns (Mode value) {
        assembly ("memory-safe") {
            value := 4
        }
    }

    function structConstructor() external pure returns (Mode) {
        return Value(dirty()).mode;
    }

    function arrayLiteral() external pure returns (Mode) {
        Mode[1] memory values = [dirty()];
        return values[0];
    }

    function arrayElement() external pure returns (Mode) {
        Mode[] memory values = new Mode[](1);
        values[0] = dirty();
        return values[0];
    }
}
