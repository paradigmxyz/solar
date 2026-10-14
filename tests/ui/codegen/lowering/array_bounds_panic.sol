//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: memFix 0 => 0
//@ run-call: memFix 1 => 20
//@ run-call: memFix 2 => 0
//@ run-call: memFixConst => 30
//@ run-call: memDyn 3, 2 => 0
//@ run-call: stFix 2 => 0
//@ run-call-fail: memFix 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: memFixConstOob => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: memDyn 3, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: memDyn 0, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: stDyn 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: stDynWrite 0, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: stFix 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Array indexing emits a bounds check that reverts with Panic(0x32)
// (selector 0x4e487b71, code 0x32) when `index >= length`, matching solc:
// - fixed-size arrays check against their compile-time length;
// - dynamic memory arrays and memory bytes check against the length word at
//   the base pointer;
// - storage dynamic arrays check against the length stored at the base slot;
// - calldata dynamic arrays/bytes check against the length word at
//   `4 + head`;
// - constant indexes remain explicit in the unoptimized MIR.
contract ArrayBoundsPanic {
    uint256[] sdyn;
    uint256[3] sfix;

    // CHECK-LABEL: fn @memFix{{[( ]}}
    // CHECK: [[X:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: {{v[0-9]+}} = lt arg0, 3
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    // CHECK-NEXT: [[OFF1:v[0-9]+]] = shl 5, arg0
    // CHECK-NEXT: [[ELEM:v[0-9]+]] = ptradd [[X]], [[OFF1]]
    // CHECK-NEXT: {{v[0-9]+}} = load [[ELEM]]
    function memFix(uint256 i) public pure returns (uint256) {
        uint256[3] memory x;
        x[1] = 20;
        return x[i];
    }

    // CHECK-LABEL: fn @memFixConst{{[( ]}}
    // CHECK: [[X:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: lt 2, 3
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    // CHECK: lt 2, 3
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    // CHECK-NEXT: [[ELEM:v[0-9]+]] = ptradd [[X]], 64
    // CHECK-NEXT: {{v[0-9]+}} = load [[ELEM]]
    function memFixConst() public pure returns (uint256) {
        uint256[3] memory x;
        x[2] = 30;
        return x[2];
    }

    // CHECK-LABEL: fn @memFixConstOob{{[( ]}}
    // CHECK: [[X:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: lt 5, 3
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    // CHECK-NEXT: [[ELEM:v[0-9]+]] = ptradd [[X]], 160
    // CHECK-NEXT: {{v[0-9]+}} = load [[ELEM]]
    function memFixConstOob() public pure returns (uint256) {
        uint256[3] memory x;
        return x[5];
    }

    // CHECK-LABEL: fn @memDyn{{[( ]}}
    // CHECK: [[ARR:v[0-9]+]] = alloc memoryarray<1>
    // CHECK: [[LEN:v[0-9]+]] = load [[ARR]]
    // CHECK: {{v[0-9]+}} = lt arg1, [[LEN]]
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    function memDyn(uint256 n, uint256 i) public pure returns (uint256) {
        uint256[] memory x = new uint256[](n);
        return x[i];
    }

    // CHECK-LABEL: fn @stDyn{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = sload 0
    // CHECK: {{v[0-9]+}} = lt arg0, [[LEN]]
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    // CHECK: {{v[0-9]+}} = storage_array_data_slot 0
    // CHECK: {{v[0-9]+}} = add {{v[0-9]+}}, arg0
    function stDyn(uint256 i) public view returns (uint256) {
        return sdyn[i];
    }

    // CHECK-LABEL: fn @stDynWrite{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = sload 0
    // CHECK: {{v[0-9]+}} = lt arg0, [[LEN]]
    // CHECK: sstore {{v[0-9]+}}, arg1
    function stDynWrite(uint256 i, uint256 v) public {
        sdyn[i] = v;
    }

    // CHECK-LABEL: fn @stFix{{[( ]}}
    // CHECK: {{v[0-9]+}} = lt arg0, 3
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    // CHECK: sload
    function stFix(uint256 i) public view returns (uint256) {
        return sfix[i];
    }

    // CHECK-LABEL: fn @cdDyn{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK: {{v[0-9]+}} = lt arg1, [[LEN]]
    // CHECK: load calldata
    function cdDyn(uint256[] calldata x, uint256 i) public pure returns (uint256) {
        return x[i];
    }

    // CHECK-LABEL: fn @cdFix{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK: {{v[0-9]+}} = lt arg1, [[LEN]]
    // CHECK: load calldata
    function cdFix(uint256[3] calldata x, uint256 i) public pure returns (uint256) {
        return x[i];
    }

    // CHECK-LABEL: fn @cdBytes{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK: {{v[0-9]+}} = lt arg1, [[LEN]]
    // CHECK: load calldata
    function cdBytes(bytes calldata b, uint256 i) public pure returns (bytes1) {
        return b[i];
    }
}
