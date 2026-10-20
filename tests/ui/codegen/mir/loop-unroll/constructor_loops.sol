//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: get 3; constructor=[10] => 0x2b
//@ run-call: get 7; constructor=[10] => 0x11
//@ run-call-fail: get 8; constructor=[10] => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: sum 5; constructor=[10] => 35

contract Constructor {
    uint256[8] arr;
    mapping(uint256 => uint256) m;

    // Constructor code runs once per deployment and every byte of it costs calldata, so copying its
    // loops grows the deployment even at these runs. The constructor keeps both loops as written.
    // CHECK-LABEL: fn @constructor
    // CHECK-COUNT-2: = phi [{{bb[0-9]+}}: 0], {{\[}}{{bb[0-9]+}}: {{v[0-9]+}}]
    // CHECK-NOT: phi
    constructor(uint256 s) {
        for (uint256 i; i < 8; ++i) arr[i] = s + i;
        for (uint256 i; i < 6; ++i) m[i] = s * i;
    }

    // CHECK-LABEL: fn @get
    function get(uint256 i) external view returns (uint256) {
        return arr[i] + m[i];
    }

    // `sum` still copies its loop.
    // CHECK-LABEL: fn @sum
    // CHECK: add {{v[0-9]+}}, 3
    function sum(uint256 n) external pure returns (uint256 s) {
        for (uint256 i; i < n; ++i) s += i * 3 + 1;
    }
}
