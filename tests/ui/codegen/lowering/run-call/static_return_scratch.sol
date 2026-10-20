//@ codegen-matrix: standard
//@ run-call: one 5 => 6
//@ run-call: two 5 => 5, 6
//@ run-call: three 5 => 5, 6, 7
//@ run-call: unused 5 =>
//@ run-call: swapped => 2, 1
//@ run-call: aliasedArrays => [2], [1]
//@ run-call: scalarThenAlias => 5, [7]
//@ run-call: four 5 => 5, 6, 7, 8
//@ run-call: pointerWords => 0, 128, 0, 128
//@ run-call: modified 5 => 5, 6, 7
//@ run-call: structFields => 3, 2, 1
//@ run-call: shortString => "abcdefghijklmnopqrstuvwxyz012345"
//@ run-call: longString => "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-+"

// Static results of at most four words made of stack scalars, and constant
// `string` and `bytes` results of at most 64 bytes, are encoded at address 0,
// over the free memory pointer and the zero slot. `return` ends execution, so
// nothing reads them again, and every returned word is computed before the
// first store. Results with memory aggregates keep the buffer at `0x80`: their
// source can alias scratch.
contract StaticReturnScratch {
    modifier clobber() {
        _;
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            mstore(0x40, 3)
            mstore(0x60, 4)
        }
    }

    function one(uint256 x) external pure returns (uint256) {
        return x + 1;
    }

    function two(uint256 x) external pure returns (uint256, uint256) {
        return (x, x + 1);
    }

    function three(uint256 x) external pure returns (uint256, uint256, uint256) {
        return (x, x + 1, x + 2);
    }

    function unused(uint256) external pure {}

    function swapped() external pure returns (uint256 x, uint256 y) {
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            x := mload(0x20)
            y := mload(0)
        }
    }

    function aliasedArrays() external pure returns (uint256[1] memory p, uint256[1] memory q) {
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            p := 0x20
            q := 0x00
        }
    }

    function scalarThenAlias() external pure returns (uint256 x, uint256[1] memory q) {
        assembly {
            mstore(0, 7)
            q := 0x00
        }
        x = 5;
    }

    function four(uint256 x) external pure returns (uint256, uint256, uint256, uint256) {
        return (x, x + 1, x + 2, x + 3);
    }

    // The words loaded from the free memory pointer and the zero slot are overwritten by the
    // later words of the result.
    function pointerWords() external pure returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        assembly {
            a := mload(0x60)
            b := mload(0x40)
            c := mload(0x60)
            d := mload(0x40)
        }
    }

    function modified(uint256 x) external pure clobber returns (uint256 a, uint256 b, uint256 c) {
        a = x;
        b = x + 1;
        c = x + 2;
    }

    struct Triple {
        uint256 a;
        uint256 b;
        uint256 c;
    }

    // The fields are loaded before the first store, although the struct lies in the result.
    function structFields() external pure returns (uint256, uint256, uint256) {
        Triple memory t;
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            mstore(0x40, 3)
            t := 0
        }
        return (t.c, t.b, t.a);
    }

    function shortString() external pure returns (string memory) {
        return "abcdefghijklmnopqrstuvwxyz012345";
    }

    function longString() external pure returns (string memory) {
        return "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-+";
    }
}
