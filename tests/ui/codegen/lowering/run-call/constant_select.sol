//@ codegen-matrix: standard ir irsize
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --check-prefix=EVM
//@[irsize] compile-flags: -Osize -Zdump=evm-ir-runtime
//@[irsize] filecheck: --check-prefix=SIZE
//@[mir] filecheck:
//@ run-call: store 0x => 0x
//@ run-call: store 0x123456 => 0x123456
//@ run-call: store 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: word 0x => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: word 0x123456 => 0x1234560000000000000000000000000000000000000000000000000000000000
//@ run-call: word 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: prefix 0x => 0x00000000
//@ run-call: prefix 0x123456 => 0x12345600
//@ run-call: prefix 0x123456789a => 0x12345678
//@ run-call: signature true => 0x26121ff0
//@ run-call: signature false => 0xe2179b8e
//@ run-call: dirtySignature 0 => 0xe2179b8e
//@ run-call: dirtySignature 2 => 0x26121ff0
//@ run-call: dirtySignature 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x26121ff0
//@ run-call: reverseSignature 0 => 0x26121ff0
//@ run-call: reverseSignature 2 => 0xe2179b8e
//@ run-call: reverseSignature 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xe2179b8e

//@ run-call: dirtyTernary 0, 19 => 7
//@ run-call: dirtyTernary 2, 19 => 19
//@ run-call: dirtyTernary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 19 => 19
//@ run-call: yulBranch 0, 19 => 7
//@ run-call: yulBranch 2, 19 => 19

//@ run-call: choose 0, 19, 7 => 7
//@ run-call: choose 1, 19, 7 => 19
//@ run-call: choose 2, 19, 7 => 19
//@ run-call: choose 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 7 => 0
//@ run-call: choose 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 7 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: choose 0, 7, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

// EVM-LABEL: @module ConstantSelect_runtime
// SIZE-LABEL: @module ConstantSelect_runtime
// SIZE: push 0x59834352
// SIZE-NEXT: eq
// SIZE-NEXT: push [[SIGNATURE:bb[0-9]+]]
contract ConstantSelect {
    bytes saved;

    function choose(uint256 raw, uint256 yes, uint256 no) external pure returns (uint256) {
        bool condition;
        assembly { condition := raw }
        return condition ? yes : no;
    }


    // CHECK-LABEL: fn @store(
    function store(bytes memory value) external returns (bytes memory) {
        saved = value;
        return saved;
    }

    // CHECK-LABEL: fn @word(
    // CHECK: select {{.*}}, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
    function word(bytes calldata value) external pure returns (bytes32) {
        return bytes32(value);
    }

    // CHECK-LABEL: fn @prefix(
    // CHECK: select
    function prefix(bytes calldata value) external pure returns (bytes4) {
        return bytes4(value);
    }

    // CHECK-LABEL: fn @signature(
    // CHECK: select
    // SIZE: [[SIGNATURE]]:
    // SIZE: jump [[VALIDATED:bb[0-9]+]]
    // SIZE-NEXT: [[VALIDATED]]:
    // SIZE-NEXT: lt
    // SIZE-NEXT: push {{bb[0-9]+}}
    // SIZE-NEXT: jumpi
    // SIZE-NEXT: push 0xe2179b8e
    // SIZE-NEXT: push 224
    // SIZE-NEXT: shl
    // SIZE-NEXT: push 4
    // SIZE-NEXT: calldataload
    // SIZE-NEXT: iszero
    // SIZE-NEXT: iszero
    // SIZE-NEXT: push 0x26121ff0
    // SIZE-NEXT: jump [[SELECT:bb[0-9]+]]
    // SIZE-NEXT: [[SELECT]]:
    // SIZE-NEXT: push 224
    // SIZE-NEXT: shl
    // SIZE-NEXT: dup 3
    // SIZE-NEXT: swap 1
    // SIZE-NEXT: sub
    // SIZE-NEXT: mul
    // SIZE-NEXT: add
    // EVM: push 0xbc057b9e
    // EVM-NEXT: push 224
    // EVM-NEXT: shl
    // EVM-NEXT: mul
    // EVM-NEXT: push 0x26121ff0
    // EVM-NEXT: push 224
    // EVM-NEXT: shl
    // EVM-NEXT: jump {{bb[0-9]+}}
    function signature(bool condition) external pure returns (bytes memory) {
        return abi.encodeWithSignature(condition ? "f()" : "g()");
    }

    // CHECK-LABEL: fn @dirtySignature(
    // CHECK: select
    function dirtySignature(uint256 raw) external pure returns (bytes memory) {
        bool condition;
        assembly { condition := raw }
        return abi.encodeWithSignature(condition ? "f()" : "g()");
    }

    // CHECK-LABEL: fn @reverseSignature(
    // CHECK: select
    function reverseSignature(uint256 raw) external pure returns (bytes memory) {
        bool condition;
        assembly { condition := raw }
        return abi.encodeWithSignature(condition ? "g()" : "f()");
    }
    // CHECK-LABEL: fn @dirtyTernary(
    function dirtyTernary(uint256 raw, uint256 x) external pure returns (uint256) {
        bool condition;
        assembly { condition := raw }
        return condition ? x : 7;
    }

    // CHECK-LABEL: fn @yulBranch(
    function yulBranch(uint256 raw, uint256 x) external pure returns (uint256 result) {
        assembly {
            result := 7
            if raw { result := x }
        }
    }
}
