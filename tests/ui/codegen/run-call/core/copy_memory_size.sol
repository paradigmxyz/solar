//@ revisions: cancun cancun_portable paris paris_portable
//@ compile-flags: -Onone
//@[cancun] compile-flags: --evm-version=cancun
//@[cancun_portable] compile-flags: --evm-version=cancun -Zno-core-intrinsics
//@[paris] compile-flags: --evm-version=paris
//@[paris_portable] compile-flags: --evm-version=paris -Zno-core-intrinsics
//@ run-call: copy2 => 32
//@ run-call: copyBackwards => 32
//@ run-call: copy1 => 0
//@ run-call: copy0 => 0

// Optimized builds diagnose MSIZE as inexact, so compare these at -O none.
import {Bytes} from "solar:core/Bytes.sol";

contract Test {
    function copy2() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        b[32] = 0xaa;
        b[33] = 0xbb;
        uint256 growth;
        assembly { growth := msize() }
        Bytes.copyInto(b, 0, b, 32, 2);
        assembly { growth := sub(msize(), growth) }
        assert(b[0] == 0xaa && b[1] == 0xbb);
        return growth;
    }

    function copyBackwards() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        b[32] = 0xaa;
        b[33] = 0xbb;
        uint256 growth;
        assembly { growth := msize() }
        Bytes.copyInto(b, 33, b, 32, 2);
        assembly { growth := sub(msize(), growth) }
        assert(b[33] == 0xaa && b[34] == 0xbb);
        return growth;
    }

    function copy1() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        Bytes.copyInto(b, 0, b, 32, 1);
        assembly { growth := sub(msize(), growth) }
        return growth;
    }

    function copy0() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        Bytes.copyInto(b, 0, b, 64, 0);
        assembly { growth := sub(msize(), growth) }
        return growth;
    }
}
