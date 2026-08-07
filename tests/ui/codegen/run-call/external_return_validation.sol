//@ run-call-fail: ExternalReturnValidation::short()
//@ run-call-fail: ExternalReturnValidation::dirty()
//@ run-call: ExternalReturnValidation::dirtyValue() => 0
//@ run-call: ExternalReturnValidation::dirtyBool() => true
//@ run-call: ExternalReturnValidation::dirtyStruct() => (0, true)
//@ run-call: ExternalReturnValidation::dirtyArray() => [true, true]
//@ run-call: ExternalReturnValidation::dirtyMemoryFixedArray() => true
//@ run-call: ExternalReturnValidation::dirtyMemoryDynamicArray() => true
//@ run-call: ExternalReturnValidation::dirtyMemoryStruct() => true
//@ run-call: ExternalReturnValidation::dirtyMemoryLvalue() => true
//@ run-call-fail: ExternalReturnValidation::dirtyEnum() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: ExternalReturnValidation::dirtyEnumStorage() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: ExternalReturnValidation::dirtyEnumStorageRead() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: ExternalReturnValidation::dirtyEnumExternalArg() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: ExternalReturnValidation::dirtyEnumStructExternalArg() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
// ported-from: test/libsolidity/semanticTests/viaYul/dirty_memory_static_array.sol
// ported-from: test/libsolidity/semanticTests/viaYul/dirty_memory_dynamic_array.sol
// ported-from: test/libsolidity/semanticTests/reverts/invalid_enum_as_external_ret.sol
// ported-from: test/libsolidity/semanticTests/reverts/invalid_enum_stored.sol
// ported-from: test/libsolidity/semanticTests/reverts/invalid_enum_as_external_arg.sol

contract ExternalReturnValidation {
    struct Pair {
        uint8 value;
        bool flag;
    }

    struct Scalar {
        uint8 value;
    }

    struct EnumPair {
        State state;
    }

    enum State {
        A,
        B
    }

    State state;

    function short() external view returns (uint256) {
        return this.shortTarget();
    }

    function shortTarget() external pure returns (uint256) {
        assembly {
            return(0, 0)
        }
    }

    function dirty() external view returns (uint8) {
        return this.dirtyTarget();
    }

    function dirtyTarget() external pure returns (uint8) {
        assembly {
            mstore(0, 0x100)
            return(0, 32)
        }
    }

    function dirtyValue() external pure returns (uint8) {
        uint8 value;
        assembly {
            value := 0x100
        }
        return value;
    }

    function dirtyBool() external pure returns (bool) {
        bool value;
        assembly {
            value := 2
        }
        return value;
    }

    function dirtyStruct() external pure returns (Pair memory pair) {
        assembly {
            mstore(pair, 0x100)
            mstore(add(pair, 0x20), 2)
        }
    }

    function dirtyArray() external pure returns (bool[] memory values) {
        values = new bool[](2);
        assembly {
            mstore(add(values, 0x20), 2)
            mstore(add(values, 0x40), 0x100)
        }
    }

    function dirtyMemoryFixedArray() external pure returns (bool) {
        uint8[1] memory values;
        assembly {
            mstore(values, 0x101)
        }
        uint8 value = values[0];
        uint256 raw;
        assembly {
            raw := value
        }
        return value == 1 && raw == 1;
    }

    function dirtyMemoryDynamicArray() external pure returns (bool) {
        uint8[] memory values = new uint8[](1);
        assembly {
            mstore(add(values, 0x20), 0x102)
        }
        uint8 value = values[0];
        uint256 raw;
        assembly {
            raw := value
        }
        return value == 2 && raw == 2;
    }

    function dirtyMemoryStruct() external pure returns (bool) {
        Scalar memory value;
        assembly {
            mstore(value, 0x101)
        }
        uint8 field = value.value;
        uint256 raw;
        assembly {
            raw := field
        }
        return field == 1 && raw == 1;
    }

    function dirtyMemoryLvalue() external pure returns (bool) {
        uint8[1] memory values;
        assembly {
            mstore(values, 0x101)
        }
        values[0] += 1;
        return values[0] == 2;
    }

    function dirtyEnum() external pure returns (State value) {
        assembly {
            value := 2
        }
    }

    function dirtyEnumStorage() external returns (uint256) {
        State value;
        assembly {
            value := 2
        }
        state = value;
        return 1;
    }

    function dirtyEnumStorageRead() external returns (uint256) {
        assembly {
            sstore(0, 2)
        }
        return uint256(state);
    }

    function dirtyEnumExternalArg() external view returns (uint256) {
        State value;
        assembly {
            value := 2
        }
        return this.enumTarget(value);
    }

    function enumTarget(State) external pure returns (uint256) {
        return 1;
    }

    function dirtyEnumStructExternalArg() external view returns (uint256) {
        EnumPair memory pair;
        assembly {
            mstore(pair, 2)
        }
        return this.enumStructTarget(pair);
    }

    function enumStructTarget(EnumPair memory) external pure returns (uint256) {
        return 1;
    }
}
