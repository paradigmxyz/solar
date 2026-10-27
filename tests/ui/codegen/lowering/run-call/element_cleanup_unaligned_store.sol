//@ codegen-matrix: standard
//@ run-call: unaligned 5 => 288230376151711751
//@ run-call: forged 5 => 288230376151711751

// A store of a word no wider than the one it replaces keeps an array's elements narrow only at
// an element's own word. One byte past it, the store's low byte lands in the next element's top
// byte, so that element's read still masks its word. So it does when a callee stores at an
// element word of a pointer that assembly moved one byte into the array.
contract ElementCleanupUnalignedStore {
    function unaligned(uint8 x) external pure returns (uint256) {
        uint8[] memory a = new uint8[](2);
        a[0] = x;
        a[1] = 7;
        uint256 at;
        uint256 old;
        assembly {
            at := add(a, 33)
            old := mload(at)
        }
        uint256 next;
        unchecked {
            next = old - 1;
        }
        if (next < old) {
            assembly {
                mstore(at, next)
            }
        }
        return uint256(a[1]) + (uint256(a[0]) << 56);
    }

    function forged(uint8 x) external pure returns (uint256) {
        uint8[] memory a = new uint8[](2);
        a[0] = x;
        a[1] = 7;
        uint8[] memory shifted;
        assembly {
            shifted := add(a, 1)
        }
        _lower(shifted);
        return uint256(a[1]) + (uint256(a[0]) << 56);
    }

    function _lower(uint8[] memory b) private pure {
        uint256 old;
        assembly {
            old := mload(add(b, 32))
        }
        uint256 next;
        unchecked {
            next = old - 1;
        }
        if (next < old) {
            assembly {
                mstore(add(b, 32), next)
            }
        }
    }
}
