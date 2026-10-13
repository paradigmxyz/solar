//@ compile-flags: -O gas -Zdump=mir
//@ filecheck:
//@ run-call: viaHelper 0x => true
//@ run-call: viaHelper 0x61 => true
//@ run-call: viaHelper 0x80 => false
//@ run-call: viaHelper 0x616161616161616161616161616161616161616161616161616161616161616161 => true
//@ run-call: viaHelper 0x616161616161616161616161616161616161616161616161616161616161616180 => false
//@ run-call: viaHelper 0x6161616161616161616161616161616161616161616161616161616161616161616161616161616180616161616161616161616161616161616161616161616161 => false

// A scan in an internal helper that is inlined into its only ABI wrapper is
// bounded by the length decoding stored, which forwarding hands the loop in
// place of a load from the object. The 7-bit scan still becomes a reduction
// over whole words.
contract InlinedScan {
    function isAscii(string memory s) internal pure returns (bool) {
        bytes memory b = bytes(s);
        for (uint256 i; i < b.length; ++i) {
            if (b[i] > 0x7f) return false;
        }
        return true;
    }

    // CHECK-LABEL: fn @viaHelper{{[( ]}}
    // CHECK: 0x8080808080808080808080808080808080808080808080808080808080808080
    function viaHelper(bytes memory b) external pure returns (bool) {
        return isAscii(string(b));
    }
}
