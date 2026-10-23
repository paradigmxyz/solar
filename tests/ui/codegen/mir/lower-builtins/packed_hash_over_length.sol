//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: signed 0x616263 => 0xe28f5ff58ff3f1b24d6ba6e3b3e95e49589e8dd59b91296e76189d6ad2857b22
//@ run-call: signed 0x => 0x5f35dce98ba4fba25530a026ed80b2cecdaa31091ba4958b99b52ea1d068adad
//@ run-call: kept 0x0102 => 0x655007acb165178ffdbe60785d761eb5334ad1cbae6e93e223db6047784bfa8a, 0x0102
//@ run-call: narrow 0xff => 0x568d2c14755c54a81b2e1a7065acce3776e67c1f6d0875b26af845c57789aa2d
//@ run-call: empty => 0xa6885b3731702da62e8e4a8f584ac46a7f6822f4e2ba50fba902f67b1588d23b, 0
//@ run-call: whole 0x0102 => 0x22ae6da6b482f9b1b19b0b897c3fd43884180a1c5ee361e1107a1bc635649dda
//@ run-call: tooLong 0x01 => 0xedbba8fe38166fa9b4934b1eab78a621aa4bc33268a304ee404c2268447c3a4c

// The hash of literals and scalars of at most a word followed by bytes in memory reads the
// bytes where they are: the prefix goes over the last bytes of their length word, which is
// written back after the hash, so the bytes are not copied.
contract PackedHashOverLength {
    // CHECK-LABEL: fn @signed(
    // CHECK-NOT: mcopy
    // CHECK: [[HEADER:v[0-9]+]] = or {{v[0-9]+}}, 0x19457468657265756d205369676e6564204d6573736167653a0a00
    // CHECK-NEXT: mstore [[BYTES:v[0-9]+]], [[HEADER]]
    // CHECK-NOT: mcopy
    // CHECK: [[START:v[0-9]+]] = add [[BYTES]], 5
    // CHECK-NEXT: keccak256 [[START]],
    // CHECK-NEXT: mstore [[BYTES]],
    function signed(bytes memory s) external pure returns (bytes32) {
        require(s.length < 10);
        return keccak256(
            abi.encodePacked("\x19Ethereum Signed Message:\n", uint8(48 + s.length), s)
        );
    }

    // The bytes keep their length after the hash.
    function kept(bytes memory s) external pure returns (bytes32 h, bytes memory r) {
        h = keccak256(abi.encodePacked(uint8(7), s));
        r = s;
    }

    // A signed scalar keeps only its own bytes and fixed bytes their leading bytes.
    function narrow(bytes memory s) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(int16(-2), bytes2(0xabcd), s));
    }

    // Empty bytes may be the zero slot, which holds zero again after the hash.
    function empty() external pure returns (bytes32 h, uint256 length) {
        bytes memory e;
        h = keccak256(abi.encodePacked(bytes4(0x01020304), e));
        bytes memory f;
        length = f.length;
    }

    // CHECK-LABEL: fn @whole(
    // CHECK-NOT: mstore
    // CHECK-NOT: mcopy
    // CHECK: keccak256
    function whole(bytes memory s) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(s));
    }

    // A prefix longer than a word is copied with the bytes.
    // CHECK-LABEL: fn @tooLong(
    // CHECK: mcopy
    // CHECK: keccak256
    function tooLong(bytes memory s) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(bytes32(uint256(1)), uint8(2), s));
    }
}
