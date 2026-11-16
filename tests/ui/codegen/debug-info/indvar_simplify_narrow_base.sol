//@ compile-flags: -Ogas -Zdump=mir -Zdebugify=check
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
// Widening a narrow base for a preheader address recurrence takes the loop statement's location,
// like the other setup operations of the recurrence.
contract NarrowBase {
    // CHECK-LABEL: fn @narrowBase(
    // CHECK: add
    function narrowBase(uint8 base, uint256 extra) external pure returns (uint256 sum) {
        unchecked {
            for (uint256 i; i < 4; ++i) {
                uint256 position = uint256(base) + 32 * i + extra * 2;
                sum += position;
                sum ^= position + 7;
            }
        }
    }
}
