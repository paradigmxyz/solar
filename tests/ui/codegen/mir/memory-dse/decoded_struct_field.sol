//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: tokenId 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000009000000000000000000000000000000000000000000000000000000000000000a => 7
//@ run-call-fail: tokenId 0x000000000000000000000000000000000000000000000000000000000000000700000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000009

// Reading one field of a decoded struct leaves the other fields written to a
// struct nothing reads again. Its stores go once forwarding has replaced the
// read of `tokenId` with the decoded word; too short an input still reverts.
struct Params {
    uint256 tokenId;
    uint256 b;
    uint256 c;
    uint256 d;
}

contract DecodedStructField {
    // CHECK-LABEL: fn @tokenId
    // CHECK: alloc 128 !metadata(deferred_alloc)
    // CHECK-NOT: mstore v{{[0-9]+}}
    // CHECK: mstore 0,
    // CHECK-NEXT: returndata 0, 32
    function tokenId(bytes calldata data) external pure returns (uint256) {
        return abi.decode(data, (Params)).tokenId;
    }
}
