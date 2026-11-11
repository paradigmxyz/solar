pragma solidity ^0.8.20;

/// Erasable ghost code verifies as before: a counter and a history written only by ghost updates
/// and read only by specs, ghost updates that read real state, and lemmas whose loops carry a
/// variant.
/// @custom:ghost uint256 calls
/// @custom:ghost uint256 len
/// @custom:ghost mapping(uint256 => uint256) history
/// @custom:ghost uint256 maxSeen
/// @custom:invariant len <= calls
/// @custom:invariant forall uint256 i :: i < len ==> history[i] <= maxSeen
contract Ghosts {
    uint256 x;

    /// @custom:ensures calls == old(calls) + 1
    /// @custom:ensures len == old(len) + 1 && history[len - 1] == v
    /// @custom:ensures maxSeen >= v
    function ok_record(uint256 v) public {
        /// @custom:ghost calls = calls + 1
        x = v;
        /// @custom:ghost history[len] = v
        x = v;
        /// @custom:ghost len = len + 1
        x = v;
        /// @custom:ghost maxSeen = v > maxSeen ? v : maxSeen
        x = v;
    }

    /// A ghost update reads real state; real code never reads the ghost.
    /// @custom:ensures calls == old(calls) + x
    function ok_bump_by_x() public {
        /// @custom:ghost calls = calls + x
        x = x;
    }

    /// The ghost counter does not exist at runtime, but specs may relate it to real state.
    /// @custom:ensures x == old(x) + 1 && calls == old(calls) + 1
    function ok_count() public {
        /// @custom:ghost calls = calls + 1
        x = x + 1;
    }

    /// A ghost update may call a pure function.
    /// @custom:ensures maxSeen == Lemmas.max(old(maxSeen), v)
    function ok_ghost_pure_call(uint256 v) public {
        /// @custom:ghost maxSeen = Lemmas.max(maxSeen, v)
        x = v;
    }
}

library Lemmas {
    function max(uint256 a, uint256 b) internal pure returns (uint256) {
        return a > b ? a : b;
    }

    /// A lemma's loop needs a variant, and the lemma must return wherever its requires hold.
    /// @custom:requires a <= b
    /// @custom:ensures a * c <= b * c
    function ok_mul_mono(uint256 a, uint256 b, uint256 c) internal pure {
        uint256 i = 0;
        /// @custom:invariant i <= 3
        /// @custom:decreases 3 - i
        while (i < 3) {
            i++;
        }
    }

    /// @custom:requires x <= y
    /// @custom:ensures result <= y * k
    function ok_use(uint256 x, uint256 y, uint256 k) internal pure returns (uint256) {
        /// @custom:use ok_mul_mono(x, y, k)
        uint256 r = x * k;
        return r;
    }

    /// A lemma that always reverts would prove anything at its uses: it must return normally.
    // expect: ghost function may revert
    /// @custom:ensures false
    function bad_reverting_lemma() internal pure {
        revert();
    }

    function uses_reverting_lemma() internal pure {
        /// @custom:use bad_reverting_lemma()
        uint256 z = 0;
        z;
    }

    /// A lemma whose requires rule out its revert is total.
    /// @custom:requires a < 10
    /// @custom:ensures a + 1 <= 10
    function ok_guarded_lemma(uint256 a) internal pure {
        require(a < 10);
    }

    function uses_guarded_lemma(uint256 a) internal pure {
        require(a < 5);
        /// @custom:use ok_guarded_lemma(a)
        uint256 z = a;
        z;
    }
}
