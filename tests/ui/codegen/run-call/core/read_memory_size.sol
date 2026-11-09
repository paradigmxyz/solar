//@ revisions: intrinsic portable
//@ compile-flags: -Onone
//@[portable] compile-flags: -Zno-core-intrinsics
//@ run-call: read1 => 0
//@ run-call: read2 => 32
//@ run-call: read32 => 32
//@ run-call: readWord => 32

import {Bytes} from "solar:core/Bytes.sol";

contract Test {
    function read1() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        assert(Bytes.readBytes1(b, 32) == 0);
        assembly { growth := sub(msize(), growth) }
        return growth;
    }

    function read2() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        assert(Bytes.readBytes2(b, 32) == 0);
        assembly { growth := sub(msize(), growth) }
        return growth;
    }

    function read32() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        assert(Bytes.readBytes32(b, 32) == 0);
        assembly { growth := sub(msize(), growth) }
        return growth;
    }

    function readWord() external pure returns (uint256) {
        bytes memory b = new bytes(64);
        uint256 growth;
        assembly { growth := msize() }
        assert(Bytes.readUint256BE(b, 32) == 0);
        assembly { growth := sub(msize(), growth) }
        return growth;
    }
}
