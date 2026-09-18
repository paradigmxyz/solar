//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: doubled 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20 => 0x020406080a0c0e10121416181a1c1e20222426282a2c2e30323436383a3c3e40
//@ run-call: doubled 0x80000000000000000000000000000000000000000000000000000000000000ff2122 => 0x00000000000000000000000000000000000000000000000000000000000001fe2122
//@ run-call: doubled 0x0102 => 0x0102
//@ run-call: doubledTwice 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021 => 0x04080c1014181c2024282c3034383c4044484c5054585c6064686c7074787c8021
//@ run-call: doubled 0x => 0x

import {Bytes} from "solar:core/v1/Bytes.sol";

contract BytesWriteAlias {
    function doubled(bytes memory a) external pure returns (bytes memory) {
        return _doubled(a);
    }

    function doubledTwice(bytes memory a) external pure returns (bytes memory) {
        return _doubled(_doubled(a));
    }

    // The output is a fresh allocation, so a word written into it through the module stays
    // inside it and reaches neither the input's length word nor its words: the input's
    // length is read once for the loop bound, and the reads of the module's own bounds
    // checks fold into it instead of reloading it after each write.
    // CHECK-LABEL: fn @_doubled
    // CHECK: mload arg0
    // CHECK-NOT: mload arg0
    function _doubled(bytes memory a) private pure returns (bytes memory out) {
        out = new bytes(a.length);
        uint256 i;
        for (; i + 32 <= a.length; i += 32) {
            bytes32 word = Bytes.readBytes32(a, i);
            Bytes.writeBytes32(out, i, bytes32(uint256(word) << 1));
        }
        for (; i < a.length; ++i) {
            out[i] = a[i];
        }
    }
}
