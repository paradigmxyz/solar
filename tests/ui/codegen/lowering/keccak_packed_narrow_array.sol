//@ codegen-matrix: standard
//@ run-call: addresses() => 0x5938c2dcfae95c263f77605563b7672b0f85bb267ee6a5c0e4cf72c00527ac34
//@ run-call: bools() => 0xada5013122d395ba3c54772283fb069b10426056ef8ca54750cb9bb552a59e7d
//@ run-call: uint16s() => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6
//@ run-call: int8s() => 0x4b9470c4b865f90802b321bfd5b0e3e3cc82854cfa29111e83eb878308dbdd1c
//@ run-call: bytes4s() => 0x7226c59bee1feea810ea6a7fca1b6540a458167f6e55860209131aae72b4e0db
//@ run-call-fail: colors() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: words() => 0xb6d87b77c1e0b78fdcb3c878fd92dfe2281f40f3dcf5328e40d76970ec0a9f1a

// The packed encoding of an array cleans each element as its type requires and
// checks the range of an enum, so `keccak256(abi.encodePacked(a))` of an array
// whose elements inline assembly left dirty hashes the clean values, as solc
// 0.8.37 does, and an enum out of range panics. Only an array of full words,
// which needs no cleaning, is hashed in place.
contract Test {
    enum Color {
        Red,
        Green
    }

    function addresses() external pure returns (bytes32) {
        address[] memory a = new address[](2);
        assembly {
            mstore(add(a, 0x20), not(0))
            mstore(add(a, 0x40), 0x1234)
        }
        return keccak256(abi.encodePacked(a));
    }

    function bools() external pure returns (bytes32) {
        bool[] memory a = new bool[](2);
        assembly {
            mstore(add(a, 0x20), 2)
        }
        return keccak256(abi.encodePacked(a));
    }

    function uint16s() external pure returns (bytes32) {
        uint16[] memory a = new uint16[](1);
        assembly {
            mstore(add(a, 0x20), 0x10001)
        }
        return keccak256(abi.encodePacked(a));
    }

    function int8s() external pure returns (bytes32) {
        int8[] memory a = new int8[](2);
        assembly {
            mstore(add(a, 0x20), 0x17f)
            mstore(add(a, 0x40), 0x80)
        }
        return keccak256(abi.encodePacked(a));
    }

    function bytes4s() external pure returns (bytes32) {
        bytes4[] memory a = new bytes4[](1);
        assembly {
            mstore(add(a, 0x20), not(0))
        }
        return keccak256(abi.encodePacked(a));
    }

    function colors() external pure returns (bytes32) {
        Color[] memory a = new Color[](1);
        assembly {
            mstore(add(a, 0x20), 2)
        }
        return keccak256(abi.encodePacked(a));
    }

    function words() external pure returns (bytes32) {
        uint256[] memory a = new uint256[](2);
        assembly {
            mstore(add(a, 0x20), not(0))
            mstore(add(a, 0x40), 0x1234)
        }
        return keccak256(abi.encodePacked(a));
    }
}
