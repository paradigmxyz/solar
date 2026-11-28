// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// A counted loop whose result is a loop word, with no hand-written invariant. The built-in
// candidates (GenObl, Inv.allSeqs) give no sequents for an end that returns a computed word;
// the erasure route with a discovered invariant (loop_infer.py, GenInfer, allSeqsW) does.
contract CountTo {
    /// @custom:ensures c == n
    function count(uint256 n) external pure returns (uint256 c) {
        for (uint256 i = 0; i < n; i++) {
            c++;
        }
    }
}
