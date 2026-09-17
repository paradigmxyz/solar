//@ codegen-matrix: standard
//@ run-call: check 42, 99, 0 => 34, 42, 99
//@ run-call: check 42, 99, 32 => 46, 42, 99
//@ run-call: check 0, 0, 0 => 34, 0, 0
//@ run-call: check 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 32 => 46, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0

contract RestoredWords {
    function check(uint256 x, uint256 y, uint256 offset)
        external pure returns (uint256 sum, uint256 afterX, uint256 afterY)
    {
        bytes memory data = new bytes(64);
        assembly {
            mstore(add(data, 32), x)
            mstore(add(data, 64), y)
        }
        sum = readTemporary(data, offset) + readTemporary(data, offset);
        assembly {
            afterX := mload(add(data, 32))
            afterY := mload(add(data, 64))
        }
    }

    function readTemporary(bytes memory data, uint256 offset) internal pure returns (uint256 result) {
        assembly {
            let p := add(data, 32)
            let q := add(p, 32)
            let x := mload(p)
            let y := mload(q)
            mstore(p, 17)
            mstore(q, 23)
            result := mload(add(p, and(offset, 32)))
            mstore(q, y)
            mstore(p, x)
        }
    }
}
