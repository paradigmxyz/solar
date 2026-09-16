//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call-fail: boom 0xdeadbeef => 0xdeadbeef
//@ run-call-fail: boom 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call-fail: boom 0x
//@ run-call: maybe 0x01, false => 7
//@ run-call-fail: maybe 0x0102, true => 0x0102

// `Revert.raw` reverts with its argument as the whole payload, no encoding
// around it. The intrinsic is the revert terminator over the buffer's range;
// the shipped body is the same terminator written in assembly.
import {Revert} from "solar:core/v1/Revert.sol";

contract Test {
    function boom(bytes memory data) public pure {
        Revert.raw(data);
    }

    function maybe(bytes memory data, bool go) public pure returns (uint256) {
        if (go) Revert.raw(data);
        return 7;
    }
}
