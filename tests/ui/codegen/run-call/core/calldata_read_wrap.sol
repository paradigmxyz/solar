//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: reads 0x => true

import {CalldataBytes} from "solar:core/CalldataBytes.sol";

contract Test {
    function reads(bytes calldata input) external pure returns (bool) {
        bytes32 first;
        assembly { first := calldataload(0) input.offset := not(0) input.length := 32 }
        assert(CalldataBytes.readBytes1(input, 0) == 0);
        assert(CalldataBytes.readBytes2(input, 0) == bytes2(first >> 8));
        assert(CalldataBytes.readBytes4(input, 0) == bytes4(first >> 8));
        assert(CalldataBytes.readBytes31(input, 0) == bytes31(first >> 8));
        assert(CalldataBytes.readBytes32(input, 0) == first >> 8);
        assert(CalldataBytes.readUint256BE(input, 0) == uint256(first) >> 8);
        assert(CalldataBytes.readBytes4(input, 1) == bytes4(first));
        assembly { input.offset := sub(not(0), 1) }
        assert(CalldataBytes.readBytes4(input, 0) == bytes4(first >> 16));
        assembly { input.offset := sub(not(0), 31) }
        assert(CalldataBytes.readBytes32(input, 0) == 0);
        return true;
    }
}
