//@ run-call: readSeeded(uint256,uint256) 7, 81 => 81
//@ run-call: writeChecked(uint256,uint256) 9, 42 => 42
//@ run-call: readNestedSeeded(uint256,uint256,uint256) 3, 5, 99 => 99
//@ run-call: writeNestedChecked(uint256,uint256,uint256) 4, 6, 73 => 73

contract StorageStructMapping {
    struct Layout {
        uint256 marker;
        mapping(uint256 => uint256) values;
        mapping(uint256 => mapping(uint256 => uint256)) nested;
    }

    Layout private layout;

    function readSeeded(uint256 key, uint256 value) external returns (uint256) {
        assembly {
            mstore(0, key)
            mstore(32, 1)
            sstore(keccak256(0, 64), value)
        }
        return layout.values[key];
    }

    function writeChecked(uint256 key, uint256 value) external returns (uint256 result) {
        layout.values[key] = value;
        assembly {
            mstore(0, key)
            mstore(32, 1)
            result := sload(keccak256(0, 64))
        }
    }

    function readNestedSeeded(uint256 outer, uint256 inner, uint256 value)
        external
        returns (uint256)
    {
        assembly {
            mstore(0, outer)
            mstore(32, 2)
            let outerSlot := keccak256(0, 64)
            mstore(0, inner)
            mstore(32, outerSlot)
            sstore(keccak256(0, 64), value)
        }
        return layout.nested[outer][inner];
    }

    function writeNestedChecked(uint256 outer, uint256 inner, uint256 value)
        external
        returns (uint256 result)
    {
        layout.nested[outer][inner] = value;
        assembly {
            mstore(0, outer)
            mstore(32, 2)
            let outerSlot := keccak256(0, 64)
            mstore(0, inner)
            mstore(32, outerSlot)
            result := sload(keccak256(0, 64))
        }
    }
}
