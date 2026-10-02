//@ codegen-matrix: standard
//@ run-call: restored 42, 0 => 0, 42
//@ run-call: restored 42, 1 => 17, 42
//@ run-call: restored 123456, 8 => 136, 123456
//@ run-call: dirty 42, false => 17
//@ run-call: dirty 42, true => 42
//@ run-call: repeated 42, 8, false => 8, 8, 42
//@ run-call: repeated 42, 8, true => 8, 0, 42
//@ run-call: repeated 123456, 0, false => 0, 0, 123456
//@ run-call: same 42, 8 => 16, 42
//@ run-call: same 0, 1 => 2, 0
contract RestoredCalls {
    function same(uint256 x, uint256 n) external pure returns (uint256 sum, uint256 afterValue) {
        require(n <= 8);
        bytes memory data = new bytes(64);
        assembly { mstore(add(data, 32), x) mstore(add(data, 64), 1) }
        sum = scan(data, n) + scan(data, n);
        assembly { afterValue := mload(add(data, 32)) }
    }

    function repeated(uint256 x, uint256 n, bool change)
        external pure returns (uint256 first, uint256 second, uint256 afterValue)
    {
        require(n <= 8);
        bytes memory data = new bytes(64);
        assembly { mstore(add(data, 32), x) mstore(add(data, 64), 1) }
        first = scan(data, n);
        if (change) { assembly { mstore8(add(data, 95), 0) } }
        second = scan(data, n);
        assembly { afterValue := mload(add(data, 32)) }
    }
    function scan(bytes memory data, uint256 n) internal pure returns (uint256 result) {
        assembly {
            let p := add(data, 32)
            let saved := mload(p)
            mstore(p, 0)
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                result := add(result, mload(add(p, 32)))
            }
            mstore(p, saved)
        }
    }

    function restored(uint256 x, uint256 n) external pure returns (uint256 result, uint256 afterValue) {
        require(n <= 8);
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), x) }
        result = temporary(data, n);
        assembly { afterValue := mload(add(data, 32)) }
    }
    function temporary(bytes memory data, uint256 n) internal pure returns (uint256 result) {
        assembly {
            let p := add(data, 32)
            let saved := mload(p)
            mstore(p, 17)
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                result := add(result, mload(p))
            }
            mstore(p, saved)
        }
    }
    function dirty(uint256 x, bool restore) external pure returns (uint256 afterValue) {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), x) }
        conditional(data, restore);
        assembly { afterValue := mload(add(data, 32)) }
    }
    function conditional(bytes memory data, bool restore) internal pure {
        assembly {
            let p := add(data, 32)
            let saved := mload(p)
            mstore(p, 17)
            if restore { mstore(p, saved) }
        }
    }
}
