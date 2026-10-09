//@ codegen-matrix: standard opt
//@ run-call: single => 3
//@ run-call: pair => 6
//@ run-call-fail: short
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=MIR

// An external call that returns static aggregates decodes them in place out of a raw output
// buffer, read as a slice of the constant head size. The buffer carries no length word, and a
// single aggregate is not copied into a bytes object before decoding.
contract ExternalStaticAggregateReturn {
    struct S {
        uint256 a;
        uint256 b;
    }

    function get() external pure returns (S memory s) {
        s.a = 1;
        s.b = 2;
    }

    function getPair() external pure returns (S memory s, uint256 c) {
        s.a = 1;
        s.b = 2;
        c = 3;
    }

    function narrow() external pure returns (uint256) {
        return 1;
    }

    // MIR-LABEL: fn @single(
    // MIR: [[FMP:v[0-9]+]] = mload 64
    // MIR: add [[FMP]], 96
    // MIR: staticcall {{.*}}, 64
    // MIR-NOT: mcopy
    // MIR-NOT: mstore {{v[0-9]+}}, 64
    // MIR: returndata
    function single() external view returns (uint256) {
        S memory s = this.get();
        return s.a + s.b;
    }

    // MIR-LABEL: fn @pair(
    // MIR: [[FMP:v[0-9]+]] = mload 64
    // MIR: add [[FMP]], 128
    // MIR-NOT: mstore {{v[0-9]+}}, 96
    // MIR: staticcall {{.*}}, 96
    // MIR: returndata
    function pair() external view returns (uint256) {
        (S memory s, uint256 c) = this.getPair();
        return s.a + s.b + c;
    }

    // Returns one word where two are expected, so the returndata size check reverts.
    function short() external view returns (uint256) {
        S memory s = IWide(address(this)).narrow();
        return s.a + s.b;
    }
}

interface IWide {
    function narrow() external view returns (ExternalStaticAggregateReturn.S memory);
}
