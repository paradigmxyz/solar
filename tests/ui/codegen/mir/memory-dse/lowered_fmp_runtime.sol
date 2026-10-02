//@ codegen-matrix: standard
//@ run-call: f 0 => 0x0000000000000000000000000000000000000000000000000000000000001234
//@ run-call-fail: g 32 => 0x0000000000000000000000000000000000000000000000000000000000005678

// Assembly that moves the free memory pointer into scratch memory makes a
// read through it observe the scratch stores before it.
contract LoweredFmp {
    function setFmp(uint256 p) internal pure {
        assembly {
            mstore(0x40, p)
        }
    }

    function f(uint256 p) external pure returns (uint256) {
        setFmp(p);
        assembly {
            mstore(0x00, 0x1234)
            return(mload(0x40), 0x20)
        }
    }

    function g(uint256 p) external pure returns (uint256) {
        setFmp(p);
        assembly {
            mstore(0x20, 0x5678)
            revert(mload(0x40), 0x20)
        }
    }
}
