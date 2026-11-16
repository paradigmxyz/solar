// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// An internal call in a loop body that writes a memory array passed to it changes the caller's
// array: the loop's head must forget what it knew of that array, or a false postcondition after
// the loop verifies. Such loops are refused until the call's writes are modelled precisely.
// Every ok_* must verify, every bad_* must fail.
library MemLoopLib {
    function poke(uint256[] memory xs) internal pure {
        xs[0] = 7;
    }
}

contract MemCallLoop {
    using MemLoopLib for uint256[];

    struct S {
        uint256[] xs;
    }

    function poke(uint256[] memory xs) internal pure {
        xs[0] = 7;
    }

    function pokeAt(uint256[] memory xs, uint256 i) internal pure {
        xs[i] = 7;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == 0
    function bad_call_in_loop(uint256 n) external pure returns (uint256 r) {
        uint256[] memory a = new uint256[](1);
        for (uint256 i = 0; i < n; i++) {
            poke(a);
        }
        r = a[0];
    }

    // expect: not supported in a loop yet
    /// @custom:requires xs.length > 2
    /// @custom:ensures xs[1] == 0
    function bad_indexed_call_in_loop(uint256[] memory xs) public pure {
        xs[1] = 0;
        for (uint256 i = 0; i < xs.length; i++) {
            pokeAt(xs, i);
        }
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == 0
    function bad_library_call_in_loop(uint256 n) external pure returns (uint256 r) {
        uint256[] memory a = new uint256[](1);
        for (uint256 i = 0; i < n; i++) {
            MemLoopLib.poke(a);
        }
        r = a[0];
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == 0
    function bad_using_for_in_loop(uint256 n) external pure returns (uint256 r) {
        uint256[] memory a = new uint256[](1);
        for (uint256 i = 0; i < n; i++) {
            a.poke();
        }
        r = a[0];
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == 0
    function bad_field_in_loop(uint256 n) external pure returns (uint256 r) {
        S memory s = S(new uint256[](1));
        for (uint256 i = 0; i < n; i++) {
            poke(s.xs);
        }
        r = s.xs[0];
    }

    function twice(uint256 x) internal pure returns (uint256) {
        return 2 * x;
    }

    /// A call that takes no memory argument leaves the array known across the loop.
    /// @custom:ensures r == 5
    function ok_scalar_call_in_loop(uint256 n) external pure returns (uint256 r) {
        uint256[] memory a = new uint256[](1);
        a[0] = 5;
        for (uint256 i = 0; i < n && i < 10; i++) {
            twice(i);
        }
        r = a[0];
    }
}
