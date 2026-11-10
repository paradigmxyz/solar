//@ codegen-matrix: standard
//@ run-call: fromMemory() => 79228162514264337593543950335
//@ run-call: fromStorage() => 79228162514264337593543950335
//@ run-call: fromCalldata(address[]) [0x0000000000000000000000000000000000000003] => 79228162514264337593543950335
//@ run-call: bytes20Array() => 79228162514264337593543950335
//@ run-call: uint200Array() => 72057594037927935
//@ run-call: uint80Array() => 65535

// Dirty bits that assembly wrote above the elements of a storage array survive a copy over it;
// solc clears them and returns 0 from each function. See CODEGEN-018 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    address[] addresses;
    address[] source;
    bytes20[] words20;
    uint200[] words200;
    uint80[] words80;

    function fromMemory() external returns (uint256 r) {
        addresses.push(address(1));
        assembly {
            mstore(0, addresses.slot)
            sstore(keccak256(0, 32), not(0))
        }
        address[] memory m = new address[](1);
        addresses = m;
        assembly {
            mstore(0, addresses.slot)
            r := shr(160, sload(keccak256(0, 32)))
        }
    }

    function fromStorage() external returns (uint256 r) {
        addresses.push(address(1));
        source.push(address(2));
        assembly {
            mstore(0, addresses.slot)
            sstore(keccak256(0, 32), not(0))
        }
        addresses = source;
        assembly {
            mstore(0, addresses.slot)
            r := shr(160, sload(keccak256(0, 32)))
        }
    }

    function fromCalldata(address[] calldata c) external returns (uint256 r) {
        addresses.push(address(1));
        assembly {
            mstore(0, addresses.slot)
            sstore(keccak256(0, 32), not(0))
        }
        addresses = c;
        assembly {
            mstore(0, addresses.slot)
            r := shr(160, sload(keccak256(0, 32)))
        }
    }

    function bytes20Array() external returns (uint256 r) {
        words20.push(bytes20(0));
        assembly {
            mstore(0, words20.slot)
            sstore(keccak256(0, 32), not(0))
        }
        bytes20[] memory m = new bytes20[](1);
        words20 = m;
        assembly {
            mstore(0, words20.slot)
            r := shr(160, sload(keccak256(0, 32)))
        }
    }

    function uint200Array() external returns (uint256 r) {
        words200.push(1);
        assembly {
            mstore(0, words200.slot)
            sstore(keccak256(0, 32), not(0))
        }
        uint200[] memory m = new uint200[](1);
        words200 = m;
        assembly {
            mstore(0, words200.slot)
            r := shr(200, sload(keccak256(0, 32)))
        }
    }

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
}
