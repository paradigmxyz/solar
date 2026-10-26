//@ codegen-matrix: standard
//@ run-call: 0x749305c70000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b20000000000000000000000000000000000000000000000000000000000000000 => 0xb1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2
//@ run-call: 0x69d3a3b10000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b20000000000000000000000000000000000000000000000000000000000000000 => 0xb1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2
//@ run-call-fail: 0x749305c7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc

contract Test {
    // No function checks the size here, so the dispatcher hoists no size check out of the others.
    function bare() external pure {}

    // Each offset is checked against the room behind the pointer it adds to.
    function chained() external pure {
        assembly {
            let end := calldatasize()
            if lt(end, 100) { revert(0, 0) }
            let a := calldataload(4)
            if gt(a, sub(end, 4)) { revert(0, 0) }
            let p := add(a, 4)
            if lt(sub(end, p), 64) { revert(0, 0) }
            let b := calldataload(p)
            if gt(b, sub(sub(end, p), 32)) { revert(0, 0) }
            let arr := add(add(p, b), 32)
            let o := calldataload(arr)
            if gt(o, sub(end, arr)) { revert(0, 0) }
            let s := add(arr, o)
            if lt(sub(end, s), 96) { revert(0, 0) }
            let m := mload(0x40)
            mstore(m, calldataload(s))
            mstore(add(m, 32), calldataload(add(s, 32)))
            return(m, 64)
        }
    }

    // The same without the first check: `p` can wrap.
    function chainedLoose() external pure {
        assembly {
            let end := calldatasize()
            if lt(end, 100) { revert(0, 0) }
            let a := calldataload(4)
            
            let p := add(a, 4)
            if lt(sub(end, p), 64) { revert(0, 0) }
            let b := calldataload(p)
            if gt(b, sub(sub(end, p), 32)) { revert(0, 0) }
            let arr := add(add(p, b), 32)
            let o := calldataload(arr)
            if gt(o, sub(end, arr)) { revert(0, 0) }
            let s := add(arr, o)
            if lt(sub(end, s), 96) { revert(0, 0) }
            let m := mload(0x40)
            mstore(m, calldataload(s))
            mstore(add(m, 32), calldataload(add(s, 32)))
            return(m, 64)
        }
    }
}
