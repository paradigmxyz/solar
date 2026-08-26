//@ compile-flags: -Zsecurity --emit=bin-runtime --allow=2264

// Reducing a block value modulo a bound is predictable randomness. The analysis
// anchors on a modulo whose operand derives from block.prevrandao, blockhash,
// or block.coinbase, and flags it. block.timestamp and block.number are
// excluded because their modulo has legitimate uses (deadlines, epochs) — the
// false-positive guard.

contract WeakPrng {
    // prevrandao modulo a bound: weak randomness.
    function pickPrevrandao(uint256 n) external view returns (uint256) {
        return block.prevrandao % n; //~ WARN: randomness derived from a block value
    }

    // blockhash modulo a bound: weak randomness.
    function pickBlockhash(uint256 n) external view returns (uint256) {
        return uint256(blockhash(block.number - 1)) % n; //~ WARN: randomness derived from a block value
    }

    // Legitimate timestamp deadline: no modulo of a randomness source, no finding.
    function notExpired(uint256 deadline) external view returns (bool) {
        return block.timestamp < deadline;
    }

    // Legitimate epoch from block number (block.number is excluded): no finding.
    function epoch(uint256 length) external view returns (uint256) {
        return block.number % length;
    }
}
