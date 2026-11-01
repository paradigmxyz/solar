//@ codegen-matrix: standard
//@[gas,size] run-call: echo 17 => 17
//@[gas,size] run-call: twice 17 => 34

contract ConstantMemoryWrite {
    function reserve() external pure {
        assembly {
            return(0x80, 0x2000)
        }
    }

    function twice(uint256 value) external pure returns (uint256) {
        return value * 2;
    }

    function echo(uint256 value) external pure returns (uint256) {
        assembly {
            mstore(0x2080, 0xdeadbeef)
        }
        return value;
    }
}
