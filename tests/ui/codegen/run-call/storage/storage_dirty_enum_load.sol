//@ codegen-matrix: standard
//@ run-call-fail: unusedLocal() => Panic(0x21)
//@ run-call-fail: usedLocal() => Panic(0x21)
//@ run-call-fail: copyArray() => Panic(0x21)
//@ run-call-fail: copyStruct() => Panic(0x21)

// Reading an out-of-range enum from storage into a local panics with `Panic(0x21)` at the load;
// solc returns 1 from `unusedLocal()` and panics only at a use. A storage-to-storage copy of an
// enum array, or of a struct holding one, panics the same way where solc copies the slots and
// returns. See CODEGEN-022 in docs/SOLC_DIVERGENCE.md.
contract C {
    enum E {
        A,
        B
    }

    struct S {
        E[] values;
    }

    E e;
    E[] source;
    E[] target;
    S sourceStruct;
    S targetStruct;

    function unusedLocal() external returns (uint256) {
        assembly {
            sstore(e.slot, 5)
        }
        E x = e;
        return 1;
    }

    function usedLocal() external returns (uint256) {
        assembly {
            sstore(e.slot, 5)
        }
        E x = e;
        return uint256(x);
    }

    function copyArray() external returns (uint256) {
        source.push(E.A);
        assembly {
            mstore(0, source.slot)
            sstore(keccak256(0, 32), 5)
        }
        target = source;
        return 1;
    }

    function copyStruct() external returns (uint256) {
        sourceStruct.values.push(E.A);
        assembly {
            mstore(0, sourceStruct.slot)
            sstore(keccak256(0, 32), 5)
        }
        targetStruct = sourceStruct;
        return 1;
    }
}
