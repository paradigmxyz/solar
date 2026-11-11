//@ codegen-matrix: standard
//@ run-call: uint80Array() => 0
//@ run-call: fixedArray() => 0
//@ run-call: fixedArrayFromCalldata() => 0

// Copying a memory array over a packed storage array clears dirty bits that assembly wrote above
// its elements, like solc.
contract C {
    uint80[] words80;
    uint8[3] triple;

    function uint80Array() external returns (uint256 r) {
        words80.push(1);
        assembly {
            mstore(0, words80.slot)
            sstore(keccak256(0, 32), not(0))
        }
        uint80[] memory m = new uint80[](3);
        words80 = m;
        assembly {
            mstore(0, words80.slot)
            r := shr(240, sload(keccak256(0, 32)))
        }
    }

    function fixedArray() external returns (uint256 r) {
        assembly {
            sstore(triple.slot, not(0))
        }
        uint8[3] memory m;
        m[0] = 1;
        triple = m;
        assembly {
            r := shr(24, sload(triple.slot))
        }
    }

    function fixedArrayFromCalldata() external returns (uint256 r) {
        assembly {
            sstore(triple.slot, not(0))
        }
        this.setFixed([uint8(1), 2, 3]);
        assembly {
            r := shr(24, sload(triple.slot))
        }
    }

    function setFixed(uint8[3] calldata m) external {
        triple = m;
    }
}
