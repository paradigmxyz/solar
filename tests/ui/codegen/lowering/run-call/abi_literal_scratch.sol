//@ codegen-matrix: standard native
//@[mir] compile-flags: --evm-version paris
//@[gas] compile-flags: --evm-version paris
//@[size] compile-flags: --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun
//@ run-call: hash 7 => 0xfdea0606e3d2f11b92043cf8cb9c1c2a3500352f968a9c24773933ee35e39b06
//@ run-call: observed 7 => true
//@ run-call: recycledHash 7 => 0x9c1174e20809450043f39a4fb9f56670204a320de6e1eb8bf841d64930e1ba89
//@ run-call: text => "scratch literal that spans two words;"
//@ run-call-fail: observedRevert 7 => 0x000000000000000000000000000000000000000000000000000000000000002573637261746368206c69746572616c2074686174207370616e732074776f2077
//@ run-call-fail: fail 7 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002573637261746368206c69746572616c2074686174207370616e732074776f20776f7264732c000000000000000000000000000000000000000000000000000000
//@ run-call-fail: aliased => 0xf37cd160000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000003616263000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000036162630000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: grown 64 => 0x5c11eac3000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000406162000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000026364000000000000000000000000000000000000000000000000000000000000
//@ run-call: shifted => 0x616263, 192
//@ run-call: recycledText => 0xabcd

// A literal folded into a scratch encode: `hash` hashes it, `text` returns it, and `fail`
// reverts with it. `observed` reads the literal's allocation after the encode, and
// `observedRevert` reverts with the allocation's bytes, so both allocations must remain, and
// `shifted` returns the address of a later allocation, which the literal's must not move, and
// `aliased` reverts with a pointer to the literal's memory, which its stores must fill. `grown`
// lengthens an earlier literal, so that its copy reaches the last one. The
// recycled cases overwrite a literal through an older allocation, so its bytes differ from the
// ones it was initialized with.
error E(bytes a, string b);
error F(bytes a, bytes b, uint256 p);

contract C {
    function hash(uint256 x) external pure returns (bytes32) {
        return keccak256(abi.encode(x, "scratch literal that spans two words!"));
    }

    function observed(uint256 x) external pure returns (bool result) {
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        bytes32 digest = keccak256(abi.encode(x, "scratch literal that spans two words?"));
        assembly {
            result := and(eq(mload(before), 37), eq(mload(0x40), add(before, 96)))
            result := and(
                result, eq(mload(add(before, 32)), 0x73637261746368206c69746572616c2074686174207370616e732074776f2077)
            )
            result := and(result, eq(mload(add(before, 64)), shl(216, 0x6f7264733f)))
        }
        require(digest == 0xf7ca02e8e7052e428c9661bb6f3f0ab0e7f87756e5e2172ef9b6c9f13bf7349c);
    }

    function observedRevert(uint256 x) external pure {
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        bytes32 digest = keccak256(abi.encode(x, "scratch literal that spans two words."));
        assembly {
            revert(before, add(64, shl(5, lt(digest, 1))))
        }
    }

    function fail(uint256 x) external pure {
        if (x == 7) revert("scratch literal that spans two words,");
    }

    function aliased() external pure {
        bytes memory pointer;
        assembly {
            pointer := mload(0x40)
        }
        revert E(pointer, "abc");
    }

    function grown(uint256 n) external pure {
        bytes memory first = "ab";
        assembly {
            mstore(first, n)
        }
        bytes memory last = "cd";
        revert F(first, last, 0);
    }

    function shifted() external pure returns (bytes memory, uint256) {
        bytes memory literal = "abc";
        bytes memory next = new bytes(0);
        uint256 address_;
        assembly {
            address_ := next
        }
        return (literal, address_);
    }

    function recycledHash(uint256 x) external pure returns (bytes32) {
        bytes memory old = new bytes(32);
        assembly {
            mstore(0x40, old)
        }
        bytes memory value = hex"1111";
        assembly {
            mstore(add(old, 32), shl(240, 0xabcd))
        }
        return keccak256(abi.encode(x, value));
    }

    function text() external pure returns (string memory) {
        return "scratch literal that spans two words;";
    }

    function recycledText() external pure returns (bytes memory) {
        bytes memory old = new bytes(32);
        assembly {
            mstore(0x40, old)
        }
        bytes memory value = hex"3333";
        assembly {
            mstore(add(old, 32), shl(240, 0xabcd))
        }
        return value;
    }
}
