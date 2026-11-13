//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: ordered => 0x11223344, 12
//@ run-call: attached 0x0011223344 => 0x11223344
import {Bytes} from "solar:core/Bytes.sol";
contract Test {
    using Bytes for bytes;
    uint trace;
    function tag(uint t) internal returns(uint) { trace = trace * 10 + t; return 1; }
    function data(uint t, bytes memory b) internal returns(bytes memory) { trace = trace * 10 + t; return b; }
    function ordered() external returns(bytes4, uint) {
        bytes4 result = Bytes.readBytes4({offset: tag(1), b: data(2, hex"0011223344")});
        return (result, trace);
    }
    function attached(bytes memory b) external pure returns(bytes4) {
        return b.readBytes4({offset: 1});
    }
}
