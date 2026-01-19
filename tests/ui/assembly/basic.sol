// Test basic inline assembly parsing and lowering

contract BasicAssembly {
    function empty() public pure {
        assembly {}
    }

    function simpleOps() public pure returns (uint256 result) {
        assembly {
            let x := 1
            let y := 2
            result := add(x, y)
        }
    }

    function memoryOps() public pure returns (uint256) {
        assembly {
            mstore(0x40, 0x80)
            let ptr := mload(0x40)
            mstore(ptr, 42)
            return(ptr, 32)
        }
    }

    function storageOps() public {
        assembly {
            sstore(0, 100)
            let val := sload(0)
            sstore(1, val)
        }
    }
}
