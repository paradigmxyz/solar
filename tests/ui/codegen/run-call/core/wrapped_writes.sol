//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: writeHeader() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: copyHeader() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: distinctAliases => 64
//@ run-call: calldataWrap 0x => 0x00a1
import {CalldataBytes} from "solar:core/CalldataBytes.sol";
import {Bytes} from "solar:core/Bytes.sol";
contract Test {
    function writeHeader() external pure returns(bool) {
        bytes memory b = new bytes(64);
        assembly { mstore(b, not(0)) }
        Bytes.writeBytes4(b, type(uint).max - 31, 0x00112233);
        return true;
    }
    function copyHeader() external pure returns(bool) {
        bytes memory b = new bytes(64);
        assembly { mstore(b, not(0)) }
        Bytes.copyInto(b, type(uint).max - 31, hex"0011", 0, 2);
        return true;
    }
    function distinctAliases() external pure returns(uint) {
        bytes memory b = new bytes(96);
        bytes memory other;
        assembly { other := add(b, 32) mstore(other, 64) }
        Bytes.copyInto(other, 0, b, 0, 64);
        return uint8(b[95]);
    }
    function calldataWrap(bytes calldata data) external pure returns(bytes memory r) {
        assembly { data.offset := not(0) data.length := 2 }
        r = new bytes(2);
        CalldataBytes.copyInto(r, 0, data, 0, 2);
    }

}
