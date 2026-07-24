// Test Yul literal types

contract Literals {
    function decimalLiterals() public pure returns (uint256) {
        assembly {
            let a := 0
            let b := 1
            let c := 42
            let d := 115792089237316195423570985008687907853269984665640564039457584007913129639935

            mstore(0, d)
            return(0, 32)
        }
    }

    function hexLiterals() public pure returns (uint256) {
        assembly {
            let a := 0x0
            let b := 0x1
            let c := 0x2a
            let d := 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

            mstore(0, d)
            return(0, 32)
        }
    }

    function stringLiterals() public pure returns (bytes32) {
        assembly {
            let a := "hello"
            let b := "hello world!!!!!"

            mstore(0, a)
            return(0, 32)
        }
    }

    function hexStringLiterals() public pure returns (bytes32) {
        assembly {
            let a := hex"1234"
            let b := hex"deadbeef"

            mstore(0, a)
            return(0, 32)
        }
    }

    function booleanLiterals() public pure returns (uint256) {
        assembly {
            let t := true
            let f := false

            mstore(0, t)
            return(0, 32)
        }
    }
}
