// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Assembly memory along branches: a write under `if`, `switch` or before `leave` is seen only on
// the paths that made it. Every ok_* must verify, every bad_* must fail.
contract AsmMemPaths {
    // A write inside `if` must not reach the path that skips it.
    // expect: c = false
    function bad_if_write_leaks(bool c) public pure {
        uint256 r;
        assembly {
            mstore(0, 0)
            if c { mstore(0, 1) }
            r := mload(0)
        }
        assert(!c ? r == 1 : true);
    }

    function ok_if_write_merged(bool c) public pure {
        uint256 r;
        assembly {
            mstore(0, 0)
            if c { mstore(0, 1) }
            r := mload(0)
        }
        assert(r == (c ? 1 : 0));
    }

    // expect: k = 0
    function bad_switch_write_leaks(uint256 k) public pure {
        uint256 r;
        assembly {
            mstore(0x20, 7)
            switch k
            case 0 { }
            default { mstore(0x20, 9) }
            r := mload(0x20)
        }
        assert(r == 9);
    }

    function ok_switch_write_merged(uint256 k) public pure {
        uint256 r;
        assembly {
            mstore(0x20, 7)
            switch k
            case 0 { }
            default { mstore(0x20, 9) }
            r := mload(0x20)
        }
        assert(r == (k == 0 ? 7 : 9));
    }

    // A function that leaves before its write: the caller sees the write only when it ran.
    // expect: c = true
    function bad_leave_write_leaks(bool c) public pure {
        uint256 r;
        assembly {
            function f(flag) {
                if flag { leave }
                mstore(0, 5)
            }
            mstore(0, 3)
            f(c)
            r := mload(0)
        }
        assert(r == 5);
    }

    function ok_leave_write_merged(bool c) public pure {
        uint256 r;
        assembly {
            function f(flag) {
                if flag { leave }
                mstore(0, 5)
            }
            mstore(0, 3)
            f(c)
            r := mload(0)
        }
        assert(r == (c ? 3 : 5));
    }

    // Memory no branch wrote reads back whole after the join.
    function ok_untouched_word_after_branch(bool c, uint256 x) public pure {
        uint256 r;
        assembly {
            mstore(0, x)
            if c { mstore(0x20, 1) }
            r := mload(0)
        }
        assert(r == x);
    }

    // A one-byte write under a branch: the other bytes of the word keep their value.
    function ok_partial_write_merged(bool c, uint256 x) public pure {
        uint256 r;
        assembly {
            mstore(0, x)
            if c { mstore8(31, 0) }
            r := mload(0)
        }
        assert(c ? r == x - (x % 256) : r == x);
    }

    // expect: c = true
    function bad_partial_write_leaks(bool c, uint256 x) public pure {
        uint256 r;
        assembly {
            mstore(0, x)
            if c { mstore8(31, 0) }
            r := mload(0)
        }
        assert(r == x);
    }
}
