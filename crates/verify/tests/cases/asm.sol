// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Inline assembly (Yul): every ok_* must verify, every bad_* must fail.
contract Asm {
    uint256 constant SLOT = 0x1234;
    uint256 constant SEED = 0x87a211a2;

    struct P {
        address a;
        uint256 b;
        bool c;
    }

    // ---------------------------------------------------------------- arithmetic builtins
    function ok_add_wraps(uint256 x) public pure {
        uint256 z;
        assembly { z := add(x, 1) }
        assert(x == type(uint256).max ? z == 0 : z == x + 1);
    }

    function bad_add_wraps(uint256 x) public pure {
        uint256 z;
        assembly { z := add(x, 1) }
        assert(z > x);
    }

    function ok_sub_wraps(uint256 x, uint256 y) public pure {
        uint256 z;
        assembly { z := sub(x, y) }
        if (x >= y) assert(z == x - y);
        else assert(z == type(uint256).max - (y - x) + 1);
    }

    function ok_div_mod_by_zero(uint256 x) public pure {
        uint256 d;
        uint256 m;
        assembly {
            d := div(x, 0)
            m := mod(x, 0)
        }
        assert(d == 0 && m == 0);
    }

    function bad_div(uint256 x, uint256 y) public pure {
        uint256 z;
        assembly { z := div(x, y) }
        assert(z * y == x);
    }

    function ok_compare(uint256 x, uint256 y) public pure {
        uint256 a;
        uint256 b;
        uint256 e;
        uint256 i;
        assembly {
            a := lt(x, y)
            b := gt(x, y)
            e := eq(x, y)
            i := iszero(x)
        }
        assert(a == (x < y ? 1 : 0) && b == (x > y ? 1 : 0) && e == (x == y ? 1 : 0) && i == (x == 0 ? 1 : 0));
    }

    function ok_bitwise(uint256 x) public pure {
        uint256 z;
        uint256 o;
        uint256 n;
        assembly {
            z := and(x, 0xff)
            o := or(shl(8, and(x, 0xff)), 0x1)
            n := not(x)
        }
        assert(z == x % 256 && o == z * 256 + 1 && n == type(uint256).max - x);
    }

    function bad_bitwise(uint256 x) public pure {
        uint256 z;
        assembly { z := and(x, 0xff) }
        assert(z == x);
    }

    function ok_shr(uint256 x) public pure {
        uint256 z;
        assembly { z := shr(8, x) }
        assert(z == x / 256);
    }

    function ok_signed() public pure {
        int256 r;
        assembly { r := sdiv(sub(0, 7), 2) }
        assert(r == -3);
    }

    function ok_bool(uint256 x, uint256 y) public pure {
        bool z;
        assembly { z := xor(iszero(x), iszero(y)) }
        assert(z == ((x == 0 && y != 0) || (x != 0 && y == 0)));
    }

    // ---------------------------------------------------------------- Morpho UtilsLib idioms
    function ok_min(uint256 x, uint256 y) public pure {
        uint256 z;
        assembly { z := xor(x, mul(xor(x, y), lt(y, x))) }
        assert(z == (x < y ? x : y));
    }

    function bad_min(uint256 x, uint256 y) public pure {
        uint256 z;
        assembly { z := xor(x, mul(xor(x, y), lt(y, x))) }
        assert(z == (x > y ? x : y));
    }

    function ok_zeroFloorSub(uint256 x, uint256 y) public pure {
        uint256 z;
        assembly { z := mul(gt(x, y), sub(x, y)) }
        assert(z == (x > y ? x - y : 0));
    }

    function bad_zeroFloorSub(uint256 x, uint256 y) public pure {
        uint256 z;
        assembly { z := mul(gt(x, y), sub(x, y)) }
        assert(z > 0);
    }

    // ---------------------------------------------------------------- control flow
    function ok_revert(uint256 x) public pure returns (uint256 z) {
        assembly {
            if gt(x, 10) { revert(0, 0) }
            z := x
        }
        assert(z <= 10);
    }

    function bad_revert(uint256 x) public pure {
        assembly {
            if gt(x, 10) {
                mstore(0x00, 0xf4d678b8)
                revert(0x1c, 0x04)
            }
        }
        assert(x < 10);
    }

    function ok_functions(uint256 x) public pure {
        uint256 z;
        uint256 w;
        assembly {
            function double(v) -> r { r := add(v, v) }
            function f(v) -> r {
                r := 1
                if iszero(v) { leave }
                r := 2
            }
            z := double(x)
            w := f(x)
        }
        if (x < 2 ** 255) assert(z == 2 * x);
        assert(w == (x == 0 ? 1 : 2));
    }

    function ok_switch(uint256 x) public pure {
        uint256 z;
        assembly {
            switch x
            case 0 { z := 5 }
            case 1 { z := 7 }
            default { z := 9 }
        }
        assert(z == (x == 0 ? 5 : x == 1 ? 7 : 9));
    }

    function bad_switch(uint256 x) public pure {
        uint256 z;
        assembly {
            switch x
            case 0 { z := 5 }
            default { z := 9 }
        }
        assert(z == 5);
    }

    function bad_loop_effects(uint256 n) public pure {
        uint256 s = 0;
        for (uint256 i = 0; i < n; i++) {
            assembly { s := add(s, 1) }
        }
        assert(s == 0);
    }

    // ---------------------------------------------------------------- raw storage
    function get() public view returns (uint256 r) {
        assembly { r := sload(SLOT) }
    }

    function ok_sstore_sload(uint256 v) public {
        assembly { sstore(SLOT, v) }
        uint256 r;
        assembly { r := sload(SLOT) }
        assert(r == v && get() == v);
    }

    function bad_sload(uint256 v) public view {
        uint256 r;
        assembly { r := sload(SLOT) }
        assert(r == v);
    }

    /// @custom:ensures get() == v
    function ok_spec_view(uint256 v) public {
        assembly { sstore(SLOT, v) }
    }

    /// @custom:ensures get() == 0
    function bad_spec_view(uint256 v) public {
        assembly { sstore(SLOT, v) }
    }

    /// @custom:ensures true
    function setRaw(uint256 v) internal {
        assembly { sstore(SLOT, v) }
    }

    function bad_modset(uint256 v) public {
        uint256 a = get();
        setRaw(v);
        assert(get() == a);
    }

    // ---------------------------------------------------------------- memory and keccak
    function ok_scratch_keccak(address a, address b) public pure {
        uint256 s1;
        uint256 s2;
        assembly {
            mstore(0x0c, SEED)
            mstore(0x00, a)
            s1 := keccak256(0x0c, 0x20)
        }
        assembly {
            mstore(0x0c, or(shl(96, b), SEED))
            s2 := keccak256(0x0c, 0x20)
        }
        assert((s1 == s2) == (a == b));
        assert(s1 != SLOT);
    }

    function bad_scratch_keccak(address a, address b) public pure {
        uint256 s1;
        uint256 s2;
        assembly {
            mstore(0x0c, SEED)
            mstore(0x00, a)
            s1 := keccak256(0x0c, 0x20)
            mstore(0x00, b)
            s2 := keccak256(0x0c, 0x20)
        }
        assert(s1 != s2);
    }

    function ok_keccak_matches_solidity(uint256 x, address y) public pure {
        bytes32 h;
        assembly {
            mstore(0x00, x)
            mstore(0x20, y)
            h := keccak256(0x00, 0x40)
        }
        assert(h == keccak256(abi.encode(x, y)));
    }

    function ok_keccak_lengths(uint256 x) public pure {
        bytes32 h1;
        bytes32 h2;
        assembly {
            mstore(0x00, x)
            mstore(0x20, 0)
            h1 := keccak256(0x00, 0x20)
            h2 := keccak256(0x00, 0x40)
        }
        assert(h1 != h2);
    }

    function ok_struct_pointer(P memory p) public pure {
        bytes32 h;
        address a;
        assembly {
            h := keccak256(p, 0x60)
            a := mload(p)
        }
        assert(h == keccak256(abi.encode(p)) && a == p.a);
    }

    function bad_mload_unwritten() public pure {
        uint256 z;
        assembly { z := mload(0x00) }
        assert(z == 0);
    }
}
