//@ run-call: DirtyCarrierPaths::signedPointer => -1
//@ run-call: DirtyCarrierPaths::signedOperator => 1
//@ run-call: DirtyCarrierPaths::throughPointer => 257
//@ run-call: DirtyCarrierPaths::cleanPairDispatch => 1, 2
//@ run-call: DirtyCarrierPaths::modifierLoop
//@ run-call: DirtyCarrierPaths::signedLocal => -1
//@ run-call: DirtyVirtual::throughVirtual => 257
//@ run-call: DirtyInternalNamedReturn::dispatchAddress 0x0000000000000000000000000000000000000002 => 0x0000000000000000000000000000000000000002
//@ run-call: DirtyInternalNamedReturn::narrowAddress 0x10000000000000000000000000000000000000002 => 0x0000000000000000000000000000000000000002
//@ run-call: DirtyInternalNamedReturn::bytesAddress 0x1234567890123456789012345678901234567890 => 0x1234567890123456789012345678901234567890
//@ run-call: DirtyInternalNamedReturn::joinAddress true => 2
//@ run-call: DirtyInternalNamedReturn::joinAddress false => 0x10000000000000000000000000000000000000002
//@ codegen-matrix: standard
//@ run-call: DirtyInternalNamedReturn::addressBits => true
//@ run-call: DirtyInternalNamedReturn::bytesBits => true
//@ run-call: DirtyInternalNamedReturn::typedCallCleans => true
//@ run-call: DirtyInternalNamedReturn::directTypedCallCleans => true
//@ run-call: DirtyInternalNamedReturn::directComparisonCleans => true
//@ run-call: DirtyInternalNamedReturn::boolBits => true

contract DirtyInternalNamedReturn {
    function dispatchAddress(address value) external pure returns (address) {
        function(address) internal pure returns (address) callback = dirtyAddress;
        return callback(value);
    }

    function narrowAddress(uint256 bits) external pure returns (address) {
        return address(uint160(bits));
    }

    function bytesAddress(bytes20 bits) external pure returns (address) {
        return address(bits);
    }

    function joinAddress(bool choose) external pure returns (uint256 raw) {
        address value;
        if (choose) {
            value = address(2);
        } else {
            value = dirtyAddress(address(2));
        }
        assembly { raw := value }
    }

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

type SignedCarrier is int8;
using {signedCarrierSign as ~} for SignedCarrier global;

function signedCarrierSign(SignedCarrier value) pure returns (SignedCarrier result) {
    assembly { result := slt(value, 0) }
}

contract DirtyCarrierPaths {
    function inspectSigned(int8 value) internal pure returns (int256 raw) {
        assembly { raw := value }
    }

    function signedPointer() external pure returns (int256) {
        function(int8) internal pure returns (int256) callback = inspectSigned;
        return callback(-1);
    }

    function signedOperator() external pure returns (int8) {
        return SignedCarrier.unwrap(~SignedCarrier.wrap(-1));
    }

    function identity(uint8 x) internal pure returns (uint8) { return x; }

    function throughPointer() external pure returns (uint256 raw) {
        uint8 x;
        assembly { x := 257 }
        function(uint8) internal pure returns (uint8) callback = identity;
        uint8 y = callback(x);
        assembly { raw := y }
    }

    function cleanPair() internal pure returns (uint8, uint16) { return (1, 2); }

    function cleanPairDispatch() external pure returns (uint8, uint16) {
        function() internal pure returns (uint8, uint16) callback = cleanPair;
        return callback();
    }

    modifier rawLoop() {
        uint8 x = 1;
        for (uint256 i; i < 2; ++i) {
            assembly { x := add(x, 128) }
        }
        assembly { if iszero(eq(x, 257)) { revert(0, 0) } }
        _;
    }

    function modifierLoop() external pure rawLoop {}

    function signedLocal() external pure returns (int256 raw) {
        int8 value = -1;
        assembly { raw := value }
    }
}

abstract contract DirtyVirtualBase {
    function identity(uint8 x) internal pure virtual returns (uint8) { return x; }

    function throughVirtual() external pure returns (uint256 raw) {
        uint8 x;
        assembly { x := 257 }
        uint8 y = identity(x);
        assembly { raw := y }
    }
}

contract DirtyVirtual is DirtyVirtualBase {
    function identity(uint8 x) internal pure override returns (uint8) { return x; }
}
