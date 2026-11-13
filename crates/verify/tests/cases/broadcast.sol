// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Broadcast lemmas: `@custom:broadcast` lemmas are axioms only where `@custom:broadcast-use`
// names them, or a group that contains them, and only once they are proved.
library L {
    /// @custom:opaque
    function f(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }

    /// @custom:opaque
    function g(uint256 x) internal pure returns (uint256) {
        return 2 * x;
    }

    /// @custom:opaque
    function h(uint256 x) internal pure returns (uint256) {
        return x;
    }

    /// @custom:broadcast
    /// @custom:ensures f(x) > x
    function ok_lemma_f(uint256 x) internal pure {
        /// @custom:reveal f(x)
        return;
    }

    /// @custom:broadcast
    /// @custom:trigger g(x)
    /// @custom:requires x > 0
    /// @custom:ensures g(x) > x
    function ok_lemma_g(uint256 x) internal pure {
        /// @custom:reveal g(x)
        return;
    }

    /// A lemma proved with another broadcast lemma.
    /// @custom:broadcast
    /// @custom:broadcast-use ok_lemma_f
    /// @custom:ensures f(f(x)) > x + 1
    function ok_lemma_ff(uint256 x) internal pure {}

    /// Trusted, so never broadcast (trusted functions are not verified at all).
    /// @custom:broadcast
    /// @custom:trusted
    /// @custom:ensures h(x) > 3
    function lemma_trusted(uint256 x) internal pure {}

    /// A false lemma: its proof fails, so it is never assumed.
    /// @custom:broadcast
    /// @custom:ensures f(x) > x + 5
    function bad_lemma_false(uint256 x) internal pure {
        /// @custom:reveal f(x)
        return;
    }

    // expect: has no trigger
    /// @custom:broadcast
    /// @custom:ensures x + 0 == x
    function bad_lemma_no_trigger(uint256 x) internal pure {}

    // expect: arithmetic and comparisons cannot trigger
    /// @custom:broadcast
    /// @custom:trigger x + 1
    /// @custom:ensures f(x) > x
    function bad_lemma_arith_trigger(uint256 x) internal pure {
        /// @custom:reveal f(x)
        return;
    }

    /// Its own proof does not assume it.
    /// @custom:broadcast
    /// @custom:broadcast-use bad_lemma_self
    /// @custom:ensures h(x) > 0
    function bad_lemma_self(uint256 x) internal pure {}

    /// Two lemmas whose proofs would rest on each other: neither assumes the other.
    /// @custom:broadcast
    /// @custom:broadcast-use bad_mutual_b
    /// @custom:ensures h(x) > 1
    function bad_mutual_a(uint256 x) internal pure {}

    /// @custom:broadcast
    /// @custom:broadcast-use bad_mutual_a
    /// @custom:ensures h(x) > 2
    function bad_mutual_b(uint256 x) internal pure {}
}

/// @custom:group FG L.ok_lemma_f L.ok_lemma_g
/// @custom:group Outer FG
/// @custom:group Cyc1 Cyc2
/// @custom:group Cyc2 Cyc1
contract Scopes {
    // Dormant: nothing names the lemma here.
    // expect: postcondition not satisfied
    /// @custom:ensures L.f(a) > a
    function bad_without_broadcast(uint256 a) public pure {}

    /// @custom:broadcast-use L.ok_lemma_f
    /// @custom:ensures L.f(a) > a
    function ok_function_scope(uint256 a) public pure {}

    /// @custom:ensures L.f(a) > a
    function ok_statement_scope(uint256 a) public pure {
        /// @custom:broadcast-use L.ok_lemma_f
        /// @custom:assert L.f(a) > a
        a;
    }

    // Before the statement that names it, the lemma is not assumed.
    // expect: assertion failed
    function bad_before_statement(uint256 a) public pure {
        /// @custom:assert L.f(a) > a
        a;
        /// @custom:broadcast-use L.ok_lemma_f
        a;
    }

    /// @custom:broadcast-use FG
    /// @custom:requires a > 0
    /// @custom:ensures L.f(a) > a && L.g(a) > a
    function ok_group(uint256 a) public pure {}

    /// @custom:broadcast-use Outer
    /// @custom:requires a > 0
    /// @custom:ensures L.f(L.g(a)) > L.g(a) && L.g(a) > a
    function ok_nested_group(uint256 a) public pure {}

    // The lemma's requires guard its axiom.
    // expect: postcondition not satisfied
    /// @custom:broadcast-use FG
    /// @custom:ensures L.g(a) > a
    function bad_group_requires(uint256 a) public pure {}

    // expect: broadcast group `Cyc1` contains itself
    /// @custom:broadcast-use Cyc1
    function bad_cycle(uint256 a) public pure {}

    // expect: is not a broadcast lemma
    /// @custom:broadcast-use L.f
    function bad_not_broadcast(uint256 a) public pure {}

    // expect: broadcast lemma `bad_lemma_false` is not verified, so it is not assumed
    /// @custom:broadcast-use L.bad_lemma_false
    /// @custom:ensures L.f(a) > a + 5
    function bad_uses_false(uint256 a) public pure {}

    // expect: has no trigger
    /// @custom:broadcast-use L.bad_lemma_no_trigger
    function bad_uses_no_trigger(uint256 a) public pure {}

    // expect: cannot be trusted
    /// @custom:broadcast-use L.lemma_trusted
    /// @custom:ensures L.h(a) > 3
    function bad_uses_trusted(uint256 a) public pure {}

    /// @custom:broadcast-use L.ok_lemma_ff
    /// @custom:ensures L.f(L.f(a)) > a + 1
    function ok_uses_lemma_chain(uint256 a) public pure {}
}

/// @custom:broadcast-use L.ok_lemma_f
contract ContractScope {
    /// @custom:ensures L.f(a) > a
    function ok_contract_scope(uint256 a) public pure {}

    /// @custom:ensures L.f(L.f(a)) > a + 1
    function ok_contract_scope_twice(uint256 a) public pure {}
}
