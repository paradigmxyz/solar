//@ codegen-matrix: standard
//@ run-call: copyGroups 0, 2, 0x1234 => 0x
//@ run-call: copyGroups 2, 0, 0x1234 => 0x
//@ run-call: copyGroups 1, 1, 0x1234 => 0x0000000000000000000000000000000000000000000000000000000000001234
//@ run-call: copyGroups 2, 2, 0x1234 => 0x0000000000000000000000000000000000000000000000000000000000001234000000000000000000000000000000000000000000000000000000000000121400000000000000000000000000000000000000000000000000000000000012740000000000000000000000000000000000000000000000000000000000001254
//@ run-call-fail: copyGroups 5, 1, 0
//@ run-call-fail: copyGroups 1, 5, 0

contract NestedCopyStack {
    // The inner self-loop starts from zero. The same literal is live after it for the sentinel
    // store, while the data pointer and group offset must survive every inner iteration.
    function copyGroups(uint256 groups, uint256 words, uint256 seed)
        public pure returns (bytes memory result)
    {
        require(groups <= 4 && words <= 4);
        assembly {
            result := mload(0x40)
            let size := shl(5, mul(groups, words))
            let data := add(result, 32)
            mstore(result, size)
            for { let group := 0 } lt(group, groups) { group := add(group, 1) } {
                if words {
                    let groupOffset := shl(5, mul(group, words))
                    let j := 0
                    for {} 1 {} {
                        let offset := add(groupOffset, shl(5, j))
                        mstore(add(data, offset), xor(seed, offset))
                        j := add(j, 1)
                        if iszero(lt(j, words)) { break }
                    }
                }
            }
            mstore(add(data, size), 0)
            mstore(0x40, add(add(data, size), 32))
        }
    }

    function probe(uint256 seed) external pure returns (bytes memory) {
        return copyGroups(2, 2, seed);
    }
}
