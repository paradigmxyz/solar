// Test accessing Solidity variables from assembly

contract VariableAccess {
    uint256 public stateVar;
    uint256[] public dynamicArray;
    mapping(uint256 => uint256) public map;

    function accessLocalVar() public pure returns (uint256 result) {
        uint256 x = 42;
        assembly {
            result := x
        }
    }

    function modifyLocalVar() public pure returns (uint256 result) {
        uint256 x = 10;
        assembly {
            x := add(x, 5)
        }
        result = x;
    }

    function accessStorageSlot() public view returns (uint256 result) {
        assembly {
            result := sload(stateVar.slot)
        }
    }

    function accessArrayLength() public view returns (uint256 result) {
        assembly {
            result := sload(dynamicArray.slot)
        }
    }

    function accessMappingSlot() public pure returns (bytes32 result) {
        uint256 key = 123;
        assembly {
            mstore(0, key)
            mstore(32, map.slot)
            result := keccak256(0, 64)
        }
    }

    function accessFunctionParams(uint256 a, uint256 b) public pure returns (uint256 result) {
        assembly {
            result := add(a, b)
        }
    }

    function accessMemoryArray() public pure returns (uint256 result) {
        uint256[] memory arr = new uint256[](3);
        arr[0] = 10;
        arr[1] = 20;
        arr[2] = 30;
        assembly {
            let len := mload(arr)
            let data := add(arr, 32)
            result := mload(add(data, 64))
        }
    }
}
