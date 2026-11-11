pragma solidity ^0.8.20;
/// Ghost code that is not erasable is rejected before any solver runs, one error per broken
/// non-interference rule.
/// @custom:ghost uint256 calls
/// @custom:ghost uint256 x
contract GhostErrors {
    uint256 x;
    uint256 y;
    I token;

    function nonPure() internal returns (uint256) {
        y = 1;
        return 1;
    }

    function ghostWritesReal() external {
        /// @custom:ghost y = calls
        y = y;
    }

    function realReadsGhost() external {
        y = calls;
    }

    function ghostGuardsReal() external {
        if (calls > 0) {
            y = 1;
        }
    }

    function ghostConditionInUpdate() external {
        /// @custom:ghost calls = calls > 0 ? (y = 1) : 0
        y = y;
    }

    function ghostReverts() external {
        /// @custom:ghost require(calls > 0)
        y = y;
    }

    function ghostCallsNonPure() external {
        /// @custom:ghost calls = nonPure()
        y = y;
    }

    function ghostCallsOut() external {
        /// @custom:ghost calls = token.balanceOf(address(this))
        y = y;
    }

    function returnsGhost() external view returns (uint256) {
        return calls;
    }

    /// @custom:check calls > 0
    function checkReadsGhost() external {}

    function shadows(uint256 calls) external {
        y = calls;
    }

    function usesDivergent() external {
        /// @custom:use Lemmas.divergent()
        y = y;
    }

    function usesRecursive(uint256 n) external {
        /// @custom:use Lemmas.recursive(n)
        y = y;
    }
}

interface I {
    function balanceOf(address a) external view returns (uint256);
}

library Lemmas {
    /// A loop without a variant: the lemma would prove false by never returning.
    /// @custom:ensures false
    function divergent() internal pure {
        while (true) {}
    }

    /// @custom:ensures true
    function recursive(uint256 n) internal pure {
        if (n > 0) {
            recursive(n - 1);
        }
    }
}
