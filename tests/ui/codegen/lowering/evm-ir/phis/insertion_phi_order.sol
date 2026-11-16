//@ codegen-matrix: standard
//@ run-call: sort [] => []
//@ run-call: sort [1] => [1]
//@ run-call: sort [4, 3, 2, 1] => [1, 2, 3, 4]
//@ run-call: sort [0, 0, 1, 0] => [0, 0, 0, 1]
//@ run-call: sort [0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0] => [0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff]
contract Test {
    function sort(uint256[] memory a) external pure returns (uint256[] memory) {
        assembly {
            let n := mload(a)
            mstore(a, 0)
            let h := add(a, shl(5, n))
            for { let i := add(a, 32) } 1 {} {
                i := add(i, 32)
                if gt(i, h) { break }
                let k := mload(i)
                let j := sub(i, 32)
                let v := mload(j)
                if iszero(gt(v, k)) { continue }
                for {} 1 {} {
                    mstore(add(j, 32), v)
                    j := sub(j, 32)
                    v := mload(j)
                    if iszero(gt(v, k)) { break }
                }
                mstore(add(j, 32), k)
            }
            mstore(a, n)
        }
        return a;
    }
}
