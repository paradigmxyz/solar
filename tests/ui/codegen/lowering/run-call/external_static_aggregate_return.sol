//@ codegen-matrix: standard opt
//@[mir] filecheck:
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=OPT
//@ run-call: ExternalStaticAggregateReturn::structReturn 7 => 15
//@ run-call: ExternalStaticAggregateReturn::arrayReturn 7 => 24
//@ run-call: ExternalStaticAggregateReturn::tryStructReturn 7 => 15
//@ run-call: ExternalStaticAggregateReturn::pairReturn 7 => 24
//@ run-call-fail: ExternalStaticAggregateReturn::shortReturn

// An external call that returns static aggregates decodes them in place from
// the raw buffer it returns into, read as a slice of the constant head size, so
// the buffer has no length word.
contract ExternalStaticAggregateReturn {
    struct S {
        uint256 a;
        uint256 b;
    }

    function makeStruct(uint256 x) external pure returns (S memory) {
        return S(x, x + 1);
    }

    function makeArray(uint256 x) external pure returns (uint256[3] memory) {
        return [x, x + 1, x + 2];
    }

    function makePair(uint256 x) external pure returns (S memory, uint256) {
        return (S(x, x + 1), x + 2);
    }

    function narrow() external pure returns (uint256) {
        return 1;
    }

    // CHECK-LABEL: fn @structReturn{{[( ]}}
    // CHECK: alloc raw, exact, uninitialized, infallible, 64
    // CHECK-NOT: slice_copy
    // CHECK: [[DATA:v[0-9]+]] = make_memory_slice {{v[0-9]+}}, 64
    // CHECK: abi_decode [tuple<u256, u256>], [[DATA]]
    // OPT-LABEL: fn @structReturn{{[( ]}}
    // OPT: [[FMP:v[0-9]+]] = mload 64
    // OPT: add [[FMP]], 128
    // OPT: staticcall {{.*}}, 64
    // OPT-NOT: mcopy
    // OPT-NOT: mstore {{v[0-9]+}}, 64
    // OPT: returndata
    function structReturn(uint256 x) external view returns (uint256) {
        S memory s = this.makeStruct(x);
        return s.a + s.b;
    }

    // CHECK-LABEL: fn @arrayReturn{{[( ]}}
    // CHECK: alloc raw, exact, uninitialized, infallible, 96
    // CHECK-NOT: slice_copy
    // CHECK: [[DATA:v[0-9]+]] = make_memory_slice {{v[0-9]+}}, 96
    // CHECK: abi_decode [array<3, u256>], [[DATA]]
    function arrayReturn(uint256 x) external view returns (uint256) {
        uint256[3] memory a = this.makeArray(x);
        return a[0] + a[1] + a[2];
    }

    // CHECK-LABEL: fn @tryStructReturn{{[( ]}}
    // CHECK: alloc raw, exact, uninitialized, infallible, 64
    // CHECK-NOT: returndata_bytes
    // CHECK: [[DATA:v[0-9]+]] = make_memory_slice {{v[0-9]+}}, 64
    // CHECK: abi_decode [tuple<u256, u256>], [[DATA]]
    function tryStructReturn(uint256 x) external view returns (uint256) {
        try this.makeStruct(x) returns (S memory s) {
            return s.a + s.b;
        } catch {
            return 0;
        }
    }

    // OPT-LABEL: fn @pairReturn{{[( ]}}
    // OPT: [[FMP:v[0-9]+]] = mload 64
    // OPT: add [[FMP]], 160
    // OPT-NOT: mstore {{v[0-9]+}}, 96
    // OPT: staticcall {{.*}}, 96
    // OPT: returndata
    function pairReturn(uint256 x) external view returns (uint256) {
        (S memory s, uint256 c) = this.makePair(x);
        return s.a + s.b + c;
    }

    // Returns one word where two are expected, so the returndata size check
    // reverts.
    function shortReturn() external view returns (uint256) {
        S memory s = IWide(address(this)).narrow();
        return s.a + s.b;
    }
}

interface IWide {
    function narrow() external view returns (ExternalStaticAggregateReturn.S memory);
}
