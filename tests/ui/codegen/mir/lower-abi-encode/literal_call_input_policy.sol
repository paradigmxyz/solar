//@ revisions: gas size
//@[gas] compile-flags: -Ogas -Zdump=mir -Zmir-pipeline=lower-abi-encode
//@[size] compile-flags: -Osize -Zdump=mir -Zmir-pipeline=lower-abi-encode
//@[gas] filecheck: --check-prefixes=CHECK,GAS
//@[size] filecheck: --check-prefixes=CHECK,SIZE
//@ normalize-stdout-test: "(?s).+" -> ""

interface LiteralSink {
    function take(bytes calldata input) external;
}

contract LiteralCallInputPolicy {
    // Copy large literals directly into the ABI output, using the data-copy cost model.
    // CHECK-LABEL: fn @large(
    // CHECK-NOT: alloc
    // GAS: datacopy {{.*}}, 288
    // SIZE: datacopy {{.*}}, 257
    // CHECK-NOT: mcopy
    // CHECK: call
    function large(LiteralSink sink) external {
        sink.take(hex"ff4c675011b45284bfd6c5fe2bcc1d645a7ce02f3d932034fcfa1834fcbb207d6648e8ad98e14bce62074500c8eacb708a9d17131f56a7ad493223e07b48eee669bd83d08087875638c4dcfae97ab07797a42d0973c2e50163d9b04c237ea11661ea37bb3c209619554ba1f5b2d0b2828234fe7b0a76da0c366c4985c63501d13f62fabe2aa2579715116c491e06b707c1366a7553f90cd8066841f4772a34d456b2417df24f1a3ce47883b08aae2a8692d2c509483bebfc0cb56b41b3f6911253e95d2ad8588b39e2d8dbe2642ac81a3cef790eb832bcde2ae7bb95e8479bcc5f3c7d4a7311f7f29e4f4064c73437d9c9173f4473cb183f14136f8d3b4bbea3ff");
    }

    // Short raw literals remain visible as words for the ABI fold.
    // CHECK-LABEL: fn @small(
    // CHECK-NOT: datacopy
    // CHECK: mstore
    // CHECK: call
    function small(LiteralSink sink) external {
        sink.take(hex"010203");
    }

    // Zero payloads use a zero fill instead of adding a zero-filled data section.
    // CHECK-LABEL: fn @zero(
    // CHECK-NOT: alloc
    // CHECK: memory_zero
    // CHECK-NOT: datacopy
    // CHECK: call
    function zero(LiteralSink sink) external {
        sink.take(hex"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000");
    }
}
