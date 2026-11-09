//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: copyRight => true
//@ run-call: copyLeft => true
import {Bytes} from "solar:core/Bytes.sol";
contract Test {
    function copyRight() external pure returns (bool) {
        bytes memory source = new bytes(192);
        bytes memory dest;
        assembly { dest := add(source, 64) mstore(dest, 128) }
        for (uint256 i; i < 64; ++i) source[96 + i] = bytes1(uint8(i + 1));
        // Source starts 32 bytes before the destination, despite its larger offset.
        Bytes.copyInto(dest, 64, source, 96, 64);
        for (uint256 i; i < 64; ++i) assert(dest[64 + i] == bytes1(uint8(i + 1)));
        return true;
    }
    function copyLeft() external pure returns (bool) {
        bytes memory dest = new bytes(192);
        bytes memory source;
        assembly { source := add(dest, 64) mstore(source, 128) }
        for (uint256 i; i < 64; ++i) source[64 + i] = bytes1(uint8(i + 1));
        Bytes.copyInto(dest, 96, source, 64, 64);
        for (uint256 i; i < 64; ++i) assert(dest[96 + i] == bytes1(uint8(i + 1)));
        return true;
    }
}
