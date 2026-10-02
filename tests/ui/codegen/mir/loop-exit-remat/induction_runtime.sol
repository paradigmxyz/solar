//@ codegen-matrix: standard
//@ run-call: previous 1 => 1
//@ run-call: previous 2 => 1
//@ run-call: previous 32 => 1
//@ run-call: wrapping 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: wrapping 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: descending => 1, 2, 3, 4
contract Test {
    function previous(uint256 n) external pure returns (uint256 last) {
        assembly {
            for {} 1 {} {
                last := n
                n := sub(n, 1)
                if iszero(n) { break }
            }
        }
    }
    function wrapping(uint256 n) external pure returns (uint256 last) {
        assembly {
            for {} 1 {} {
                last := n
                n := add(n, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff)
                if eq(n, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe) { break }
            }
        }
    }
    function descending() external pure returns (uint256, uint256, uint256, uint256) {
        uint256[4] memory a = [uint256(4), 3, 2, 1];
        assembly {
            let h := add(a, 96)
            for { let i := add(a, 32) } iszero(gt(i, h)) { i := add(i, 32) } {
                let k := mload(i)
                let j := i
                for {} gt(j, a) {} {
                    let p := sub(j, 32)
                    let v := mload(p)
                    if iszero(gt(v, k)) { break }
                    mstore(j, v)
                    j := p
                }
                mstore(j, k)
            }
        }
        return (a[0], a[1], a[2], a[3]);
    }
}
