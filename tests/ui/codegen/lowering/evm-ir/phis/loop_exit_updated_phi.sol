//@ codegen-matrix: standard
//@ run-call: decimal 0 => "0"
//@ run-call: decimal 10 => "10"
//@ run-call: decimal 100 => "100"
//@ run-call: decimal 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "115792089237316195423570985008687907853269984665640564039457584007913129639935"
contract Test {
    function decimal(uint256 x) external pure returns (string memory result) {
        assembly {
            let end := add(mload(64), 128)
            mstore(64, add(end, 32))
            mstore(end, 0)
            result := end
            for {} 1 {} {
                result := sub(result, 1)
                mstore8(result, add(48, mod(x, 10)))
                x := div(x, 10)
                if iszero(x) { break }
            }
            let len := sub(end, result)
            result := sub(result, 32)
            mstore(result, len)
        }
    }
}
