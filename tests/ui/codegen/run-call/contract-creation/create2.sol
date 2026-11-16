//@ run-call: deploy => true

contract Create2 {
    function deploy() external returns (bool ok) {
        assembly {
            mstore(0, hex"60016000f3")
            ok := iszero(iszero(create2(0, 0, 5, 1)))
        }
    }
}
