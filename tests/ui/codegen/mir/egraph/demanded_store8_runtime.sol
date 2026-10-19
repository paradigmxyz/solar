//@ codegen-matrix: standard
//@ run-call: masked 0x1ff => 0xff00000000000000000000000000000000000000000000000000000000000000
//@ run-call: masked 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff12 => 0x1200000000000000000000000000000000000000000000000000000000000000
//@ run-call: masked16 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff1234 => 0x3400000000000000000000000000000000000000000000000000000000000000
//@ run-call: lowByte 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff56 => 0x5600000000000000000000000000000000000000000000000000000000000000
//@ run-call: extended 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80 => 0x8000000000000000000000000000000000000000000000000000000000000000
//@ run-call: extended 0x7ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff00000000000000000000000000000000000000000000000000000000000000
//@ run-call: shifted 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7aff => 0x7a00000000000000000000000000000000000000000000000000000000000000
//@ run-call: shiftedLeft 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x00ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: shared 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff12ab => 0xab00000000000000000000000000000000000000000000000000000000000000, 0xab
//@ run-call: shared 0x1ff => 0xff00000000000000000000000000000000000000000000000000000000000000, 0xff
//@ run-call: sharedAboveShift 0xff => 0xf000000000000000000000000000000000000000000000000000000000000000, 0xf0
//@ run-call: sharedAboveShift 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xf000000000000000000000000000000000000000000000000000000000000000, 0xf0
//@ run-call: lowNibble 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x0f00000000000000000000000000000000000000000000000000000000000000
//@ run-call: soliditySlice 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff34 => 0x34
contract DemandedStore8 {
    function masked(uint256 x) external pure returns (bytes32 word) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            mstore8(p, and(x, 0xff))
            word := mload(p)
        }
    }

    function masked16(uint256 x) external pure returns (bytes32 word) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            mstore8(p, and(x, 0xffff))
            word := mload(p)
        }
    }

    function lowByte(uint256 x) external pure returns (bytes32 word) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            mstore8(p, byte(31, x))
            word := mload(p)
        }
    }

    function extended(uint256 x) external pure returns (bytes32 word) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            mstore8(p, signextend(0, x))
            word := mload(p)
        }
    }

    function shifted(uint256 x) external pure returns (bytes32 word) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            mstore8(p, shr(8, and(x, 0xff00)))
            word := mload(p)
        }
    }

    function shiftedLeft(uint256 x) external pure returns (bytes32 word) {
        assembly {
            let p := mload(0x40)
            mstore(p, not(0))
            mstore8(p, shl(8, x))
            word := mload(p)
        }
    }

    function shared(uint256 x) external pure returns (bytes32 word, uint256 mask) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            mask := and(x, 0xff)
            mstore8(p, mask)
            word := mload(p)
        }
    }

    function sharedAboveShift(uint256 x) external pure returns (bytes32 word, uint256 low) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            low := and(shl(4, and(x, 0xf0f)), 0xfff)
            mstore8(p, low)
            word := mload(p)
        }
    }

    function lowNibble(uint256 x) external pure returns (bytes32 word) {
        assembly {
            let p := mload(0x40)
            mstore(p, 0)
            mstore8(p, and(x, 0x0f))
            word := mload(p)
        }
    }

    function soliditySlice(uint256 x) external pure returns (bytes1 first) {
        bytes memory b = new bytes(1);
        b[0] = bytes1(uint8(x));
        first = b[0];
    }
}
