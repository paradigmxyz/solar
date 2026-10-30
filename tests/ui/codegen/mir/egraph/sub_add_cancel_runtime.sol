//@ codegen-matrix: standard
//@ run-call: shared_start 0 => 32, 0
//@ run-call: shared_start 3 => 32, 96
//@ run-call: shared_start 0x0800000000000000000000000000000000000000000000000000000000000000 => 32, 0
//@ run-call: length_first 5 => 160
//@ run-call: length_first 0x0800000000000000000000000000000000000000000000000000000000000001 => 32
//@ run-call: element_count 7 => 7
//@ run-call: shared_sum 2 => 64, 64
//@ run-call: encoded_length 4 => 192

contract SubAddCancel {
    function shared_start(uint256 n) external pure returns (uint256 start, uint256 length) {
        assembly {
            let p := mload(0x40)
            start := sub(add(p, 32), p)
            length := sub(add(add(p, shl(5, n)), 32), add(p, 32))
        }
    }

    function length_first(uint256 n) external pure returns (uint256 length) {
        assembly {
            let p := mload(0x40)
            length := sub(add(add(shl(5, n), p), 32), add(p, 32))
        }
    }

    function element_count(uint256 n) external pure returns (uint256 count) {
        assembly {
            let p := mload(0x40)
            count := shr(5, sub(add(add(p, shl(5, n)), 32), add(p, 32)))
        }
    }

    function shared_sum(uint256 n) external pure returns (uint256 sum, uint256 length) {
        assembly {
            let p := mload(0x40)
            let s := add(p, shl(5, n))
            sum := sub(s, p)
            length := sub(add(s, 32), add(p, 32))
        }
    }

    function encoded_length(uint256 n) external pure returns (uint256) {
        return abi.encode(new uint256[](n)).length;
    }
}
