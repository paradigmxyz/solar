//@ revisions: cancun cancun_portable paris paris_portable
//@ compile-flags: -Onone
//@[cancun] compile-flags: --evm-version=cancun
//@[cancun_portable] compile-flags: --evm-version=cancun -Zno-core-intrinsics
//@[paris] compile-flags: --evm-version=paris
//@[paris_portable] compile-flags: --evm-version=paris -Zno-core-intrinsics
//@ run-call: write1 => 0
//@ run-call: write31 => 0
//@ run-call: write32 => 0
//@ run-call: writeWord => 0

// Optimized builds diagnose MSIZE as inexact, so compare these at -O none.
import {Bytes} from "solar:core/Bytes.sol";

contract Test {
    function write1() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        Bytes.writeBytes1(b, 63, 0xaa);
        assembly { growth := sub(msize(), growth) }
        assert(b[63] == 0xaa);
        return growth;
    }

    function write31() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        Bytes.writeBytes31(b, 33, bytes31(type(uint248).max));
        assembly { growth := sub(msize(), growth) }
        assert(b[33] == 0xff && b[63] == 0xff && b[32] == 0);
        return growth;
    }

    function write32() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        Bytes.writeBytes32(b, 32, bytes32(type(uint256).max));
        assembly { growth := sub(msize(), growth) }
        assert(b[32] == 0xff && b[63] == 0xff);
        return growth;
    }

    function writeWord() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        Bytes.writeUint256BE(b, 32, type(uint256).max);
        assembly { growth := sub(msize(), growth) }
        assert(b[32] == 0xff && b[63] == 0xff);
        return growth;
    }
}
