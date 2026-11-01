//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
// CHECK: @module
//@ run-call: wrappedRead(uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 7

contract ConstantPointerOffsetWrap {
    function wrappedRead(uint256 high) external pure returns (uint256 result) {
        uint256[3] memory words;
        assembly {
            mstore(0x40, 0xffffffffffffffe0)
            mstore(0x38, high)
            mstore(0x20, 7)
            words := mload(0x40)
        }
        result = words[2];
        assembly {
            mstore(0, result)
            return(0, 32)
        }
    }
}
