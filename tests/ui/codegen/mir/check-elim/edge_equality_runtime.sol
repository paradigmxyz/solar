//@ codegen-matrix: standard final
//@[final] compile-flags: -Ogas -Zdump=mir
//@[final] filecheck:
//@ run-call: sizes 0x1111111111111111111111111111111111111111111111111111111111111111 => 65
//@ run-call: sizes 0x3333333333 => 0
//@ run-call: sizes 0x => 0
//@ run-call: pack 0x22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222 => 0x222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222220000000000000000000000000000000000000000000000000000000000000040
//@ run-call-fail: pack 0x1111111111111111111111111111111111111111111111111111111111111111
//@ run-call-fail: pack 0x
contract EdgeEquality {
    // The length is exactly 32 after the guard, so the sum folds to a constant.
    // CHECK-LABEL: fn @sizes
    // CHECK: mstore 0, 65
    function sizes(bytes memory data) external pure returns (uint256) {
        uint256 size = data.length;
        if (size != 32) return 0;
        return size * 2 + (32 - size) + 1;
    }

    // The required length is 64, so the stored length and the padded sizes fold. The copy
    // size is a raw memory operand and stays symbolic.
    // CHECK-LABEL: fn @pack
    // CHECK: mcopy {{v[0-9]+}}, {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: mstore {{v[0-9]+}}, 64
    // CHECK: returndata {{v[0-9]+}}, 160
    function pack(bytes memory data) external pure returns (bytes memory) {
        uint256 size = data.length;
        require(size == 64);
        return abi.encodePacked(data, size);
    }
}
