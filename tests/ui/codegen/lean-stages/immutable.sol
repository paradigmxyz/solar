//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// An immutable whose placeholder is a `PUSH32` of zeros reads the word the deployment code
// writes over it, in MIR and in EVM IR alike, and the manifest lists where the placeholder is.

contract LeanStagesImmutable {
    uint256 immutable x;

    constructor(uint256 a) {
        x = a;
    }

    function f(uint256 b) external view returns (uint256) {
        return x ^ b;
    }
}
