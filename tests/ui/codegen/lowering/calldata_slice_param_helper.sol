//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=CDPARAM

// An internal helper taking a `bytes calldata` parameter used only through its
// `.offset`/`.length` in assembly. The helper inlines at every call site, but
// its call `Instruction` lingers orphaned in the arena after inlining; a later
// `lower-slices` run expands the (now dead) helper's slice parameter into a
// pointer/length pair. Validation must ignore that dead call — it is never
// emitted — instead of flagging an argument-count mismatch on it.
contract CalldataSliceParamHelper {
    // CDPARAM-LABEL: fn @lastWord
    function lastWord(bytes calldata sig) external pure returns (uint256) {
        return _last(sig);
    }

    // Two call sites — one plain, one on a sub-slice — keep the helper from
    // being trivially folded and exercise the inline/lower-slices interaction.
    // CDPARAM-LABEL: fn @lastWord2
    function lastWord2(bytes calldata sig) external pure returns (uint256) {
        return _last(sig) ^ _last(sig[1:]);
    }

    function _last(bytes calldata data) internal pure returns (uint256 x) {
        assembly {
            x := calldataload(add(data.offset, sub(data.length, 0x20)))
        }
    }
}
