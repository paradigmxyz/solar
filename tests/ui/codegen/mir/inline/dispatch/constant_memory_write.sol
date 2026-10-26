//@ codegen-matrix: standard
//@[gas,size] run-call: echo 17 => 17

contract ConstantMemoryWrite {
    function reserve() external pure {
        assembly {
            return(0x80, 0x2000)
        }
    }

    function echo(uint256 value) external pure returns (uint256) {
        assembly {
            mstore(0x2080, 0xdeadbeef)
        }
        return value;
    }
}
