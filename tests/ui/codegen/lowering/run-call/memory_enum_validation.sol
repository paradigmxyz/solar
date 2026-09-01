//@ codegen-matrix: standard
//@ run-call-fail: MemoryEnumValidation::structField => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: MemoryEnumValidation::fixedArrayElement => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: MemoryEnumValidation::dynamicArrayElement => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

contract MemoryEnumValidation {
    enum Mode {
        Zero,
        One
    }

    struct Value {
        Mode mode;
    }

    function structField() external pure returns (Mode) {
        Value memory value = Value(Mode.Zero);
        assembly ("memory-safe") {
            mstore(value, 2)
        }
        return value.mode;
    }

    function fixedArrayElement() external pure returns (Mode) {
        Mode[1] memory values;
        assembly ("memory-safe") {
            mstore(values, 2)
        }
        return values[0];
    }

    function dynamicArrayElement() external pure returns (Mode) {
        Mode[] memory values = new Mode[](1);
        assembly ("memory-safe") {
            mstore(add(values, 0x20), 2)
        }
        return values[0];
    }
}
