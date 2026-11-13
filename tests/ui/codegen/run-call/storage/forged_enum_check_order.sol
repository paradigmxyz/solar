//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: ForgedEnum::loadThenRevert => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: ForgedEnum::loadThenUse => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: ForgedEnum::cleanHighBits => 0

// An enum read from storage is checked where it is loaded. solc checks it where it is first
// used, so when assembly stores an out-of-range value and the function reverts before the use,
// solc reverts with the later error instead (docs/SOLC_DIVERGENCE.md, CODEGEN-029).

contract ForgedEnum {
    enum O { A, B, C, D, E }

    error Late();

    O o;

    function forge(uint256 raw) internal {
        assembly {
            sstore(o.slot, raw)
        }
    }

    function loadThenRevert() external returns (uint256) {
        forge(6);
        O x = o;
        if (gasleft() > 0) revert Late();
        return uint256(x);
    }

    function loadThenUse() external returns (uint256) {
        forge(0xff);
        return uint256(o);
    }

    // Only the low byte holds the enum; the bits above it are padding.
    function cleanHighBits() external returns (uint256) {
        forge(0x100);
        return uint256(o);
    }
}
