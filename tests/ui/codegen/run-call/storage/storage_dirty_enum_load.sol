//@ codegen-matrix: standard
//@ run-call-fail: unusedLocal() => Panic(0x21)
//@ run-call-fail: usedLocal() => Panic(0x21)

// Reading an out-of-range enum from storage into a local panics with `Panic(0x21)` at the load;
// solc returns 1 from `unusedLocal()` and panics only at a use. See CODEGEN-022 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    enum E {
        A,
        B
    }

    E e;

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
}
