//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// Array indexing emits a bounds check that reverts with Panic(0x32)
// (selector 0x4e487b71, code 0x32) when `index >= length`, matching solc:
// - fixed-size arrays check against their compile-time length;
// - dynamic memory arrays and memory bytes check against the length word at
//   the base pointer;
// - storage dynamic arrays check against the length stored at the base slot;
// - calldata dynamic arrays/bytes check against the length word at
//   `4 + head`;
// - constant in-range indexes emit no check at all, and constant
//   out-of-range indexes emit an unconditional panic.
// Runtime-verified differentially against solc 0.8.30 --via-ir on anvil:
// in-range results match and out-of-range reverts are byte-identical.
contract ArrayBoundsPanic {
    uint256[] sdyn;
    uint256[3] sfix;

    // CHECK-LABEL: fn @memFix{{[( ]}}
    // CHECK: {{v[0-9]+}} = lt arg0, 3
    // CHECK: mstore 4, 50
    // CHECK: memory_object_element_addr memoryfixedarray<3, 1>, {{v[0-9]+}}, arg0
    function memFix(uint256 i) public pure returns (uint256) {
        uint256[3] memory x;
        x[1] = 20;
        return x[i];
    }

    // CHECK-LABEL: fn @memFixConst{{[( ]}}
    // CHECK-NOT: mstore 4, 50
    // CHECK: returndata
    function memFixConst() public pure returns (uint256) {
        uint256[3] memory x;
        x[2] = 30;
        return x[2];
    }

    // CHECK-LABEL: fn @memFixConstOob{{[( ]}}
    // CHECK: mstore 4, 50
    // CHECK: revert 0, 36
    function memFixConstOob() public pure returns (uint256) {
        uint256[3] memory x;
        return x[5];
    }

    // CHECK-LABEL: fn @memDyn{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = memory_object_len memoryarray
    // CHECK: {{v[0-9]+}} = lt arg1, [[LEN]]
    // CHECK: mstore 4, 50
    function memDyn(uint256 n, uint256 i) public pure returns (uint256) {
        uint256[] memory x = new uint256[](n);
        return x[i];
    }

    // CHECK-LABEL: fn @stDyn{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = sload 0
    // CHECK: {{v[0-9]+}} = lt arg0, [[LEN]]
    // CHECK: mstore 4, 50
    // CHECK: keccak256 0, 32
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
    // CHECK: mstore 4, 50
    // CHECK: sload
    function stFix(uint256 i) public view returns (uint256) {
        return sfix[i];
    }

    // CHECK-LABEL: fn @cdDyn{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK: {{v[0-9]+}} = lt arg1, [[LEN]]
    // CHECK: calldataload
    function cdDyn(uint256[] calldata x, uint256 i) public pure returns (uint256) {
        return x[i];
    }

    // CHECK-LABEL: fn @cdFix{{[( ]}}
    // CHECK: {{v[0-9]+}} = lt arg3, 3
    // CHECK: memory_object_element_addr memoryfixedarray<3, 1>, {{v[0-9]+}}, arg3
    function cdFix(uint256[3] calldata x, uint256 i) public pure returns (uint256) {
        return x[i];
    }

    // CHECK-LABEL: fn @cdBytes{{[( ]}}
    // CHECK: [[LEN:v[0-9]+]] = slice_len arg0
    // CHECK: {{v[0-9]+}} = lt arg1, [[LEN]]
    // CHECK: calldataload
    function cdBytes(bytes calldata b, uint256 i) public pure returns (bytes1) {
        return b[i];
    }
}
