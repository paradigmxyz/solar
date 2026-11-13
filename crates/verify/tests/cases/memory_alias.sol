// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Memory reference types pass by reference to internal functions: a callee's writes reach the
// caller's object. Two local names for one memory object are refused once either is written.
// Every ok_* must verify, every bad_* must fail.
library MemAlias {
    function poke(uint256[] memory xs) internal pure {
        xs[0] = 7;
    }

    /// @custom:requires xs.length > 0 && xs[0] < 100
    /// @custom:ensures xs[0] == old(xs[0]) + 1
    function bump(uint256[] memory xs) internal pure {
        xs[0] = xs[0] + 1;
    }

    function peek(uint256[] memory xs) internal pure returns (uint256) {
        return xs.length > 0 ? xs[0] : 0;
    }

    // expect: assertion failed
    function bad_inlined_write(uint256[] memory xs) internal pure {
        require(xs.length > 0 && xs[0] == 1);
        poke(xs);
        assert(xs[0] == 1);
    }

    function ok_inlined_write(uint256[] memory xs) internal pure {
        require(xs.length > 0);
        poke(xs);
        assert(xs[0] == 7);
    }

    // A callee with a contract: the caller sees the value its ensures describe.
    // expect: assertion failed
    function bad_summarised_write(uint256[] memory xs) internal pure {
        require(xs.length > 0 && xs[0] == 1);
        bump(xs);
        assert(xs[0] == 1);
    }

    function ok_summarised_write(uint256[] memory xs) internal pure {
        require(xs.length > 0 && xs[0] == 1);
        bump(xs);
        assert(xs[0] == 2);
    }

    // A callee that only reads keeps the caller's facts.
    function ok_read_only_callee(uint256[] memory xs) internal pure {
        require(xs.length > 0 && xs[0] == 1);
        peek(xs);
        assert(xs[0] == 1);
    }
}

contract MemAliasStructs {
    struct S {
        uint256 a;
    }

    function pokeS(S memory s) internal pure {
        s.a = 7;
    }

    // expect: assertion failed
    function bad_struct_write() public pure {
        S memory s = S(1);
        pokeS(s);
        assert(s.a == 1);
    }

    function ok_struct_write() public pure {
        S memory s = S(1);
        pokeS(s);
        assert(s.a == 7);
    }

    // expect: aliased memory is not modelled
    function bad_local_alias() public pure {
        uint256[] memory a = new uint256[](1);
        uint256[] memory b = a;
        b[0] = 5;
        assert(a[0] == 0);
    }

    // An alias that is only read is fine.
    function ok_local_alias_read() public pure {
        uint256[] memory a = new uint256[](2);
        uint256[] memory b = a;
        assert(b.length == a.length);
    }
}
