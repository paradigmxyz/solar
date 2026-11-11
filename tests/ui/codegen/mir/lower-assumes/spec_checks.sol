//@ compile-flags: -Ogas -Zcheck-analyses=true -Zdump=mir
//@ filecheck:
//@ run-call: half 4 => 2
//@ run-call: half 255 => 127
//@ run-call-fail: half 0 => 0xed572b27000000000000000000000000000000000000020700000000000000020000000500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000

// `-Zcheck-analyses=true` tests each `@custom:ensures` clause before the return
// it is about, as it tests the facts passes rely on: fact words `0x207...` name
// `ne` (2) and a source clause (7). `half(0)` returns 0, which is not below 0,
// so the false clause reverts with `AnalysisFactViolated`, naming the record in
// bb2 about v5 and its operands' words.
contract SpecChecks {
    // CHECK-LABEL: fn @half(
    // CHECK: phi [{{.*}}: {{0x207[0-9a-f]+}}]
    // CHECK: mstore 0, 0xed572b27
    /// @custom:ensures r < x
    function half(uint8 x) external pure returns (uint8 r) {
        return x / 2;
    }
}
