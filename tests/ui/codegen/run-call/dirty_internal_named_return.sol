//@ run-call: DirtyInternalNamedReturn::addressBits() => true
//@ run-call: DirtyInternalNamedReturn::bytesBits() => true
//@ run-call: DirtyInternalNamedReturn::typedCallCleans() => true
//@ run-call: DirtyInternalNamedReturn::directTypedCallCleans() => true
//@ run-call: DirtyInternalNamedReturn::directComparisonCleans() => true
//@ run-call: DirtyInternalNamedReturn::boolBits() => true

contract DirtyInternalNamedReturn {
    function dirtyAddress(address value) internal pure returns (address result) {
        assembly {
            result := or(value, shl(160, 1))
        }
    }

    function dirtyBytes(bytes1 value) internal pure returns (bytes1 result) {
        assembly {
            result := or(value, 1)
        }
    }

    function dirtyUint8(uint8 value) internal pure returns (uint8 result) {
        assembly {
            result := or(value, shl(8, 1))
        }
    }

    function dirtyBool(bool value) internal pure returns (bool result) {
        assembly {
            result := mul(value, 2)
        }
    }

    function addressBits() external pure returns (bool dirty) {
        address value = dirtyAddress(address(2));
        assembly {
            dirty := and(eq(and(value, 0xffffffffffffffffffffffffffffffffffffffff), 2), shr(160, value))
        }
    }

    function bytesBits() external pure returns (bool dirty) {
        bytes1 value = dirtyBytes(0x42);
        assembly {
            dirty := and(eq(shr(248, value), 0x42), value)
        }
    }

    function same(address a, address b) internal pure returns (bool) {
        return a == b;
    }

    function same(bytes1 a, bytes1 b) internal pure returns (bool) {
        return a == b;
    }

    function sameWord(uint256 a, uint256 b) internal pure returns (bool) {
        return a == b;
    }

    function sameBytes32(bytes32 a, bytes32 b) internal pure returns (bool) {
        return a == b;
    }

    function typedCallCleans() external pure returns (bool) {
        address a = dirtyAddress(address(2));
        bytes1 b = dirtyBytes(0x42);
        uint8 u = dirtyUint8(3);
        return same(a, address(2)) && same(b, 0x42) && sameWord(u, 3)
            && sameBytes32(b, bytes32(bytes1(0x42)));
    }

    function directTypedCallCleans() external pure returns (bool) {
        return same(dirtyAddress(address(2)), address(2)) && same(dirtyBytes(0x42), 0x42)
            && sameWord(dirtyUint8(3), 3);
    }

    function directComparisonCleans() external pure returns (bool) {
        return dirtyAddress(address(2)) == address(2) && dirtyBytes(0x42) == bytes1(0x42)
            && dirtyUint8(3) == 3;
    }

    function boolBits() external pure returns (bool dirty) {
        bool value = dirtyBool(true);
        assembly {
            dirty := eq(value, 2)
        }
    }
}
