//@ compile-flags: -O gas -Zdump=mir
//@ filecheck:
//@ run-call: ascii 0x6161616161616161616161616161616161616161616161616161616161616161 => true
//@ run-call: ascii 0x6161616161616161616161616161616161616161616161616161616161616180 => false
//@ run-call: allowed 0x6161616180, 0x2000000000000000000000000 => false
//@ run-call: allowed 0x61616161, 0x2000000000000000000000000 => true

// The optimized pipeline matches byte scans before strength reduction turns
// their counters into pointers, so the 7-bit scan still becomes a reduction
// over whole words, while a scan the idioms leave alone walks a pointer.
contract Test {
    // CHECK-LABEL: fn @ascii{{[( ]}}
    // CHECK: 0x8080808080808080808080808080808080808080808080808080808080808080
    function ascii(bytes memory s) public pure returns (bool) {
        for (uint256 i; i < s.length; ++i) {
            if (s[i] > 0x7f) return false;
        }
        return true;
    }

    // CHECK-LABEL: fn @allowed{{[( ]}}
    // CHECK: [[POINTER:v[0-9]+]] = phi
    // CHECK-NEXT: {{v[0-9]+}} = lt [[POINTER]],
    // CHECK: {{v[0-9]+}} = mload [[POINTER]]
    // CHECK: {{v[0-9]+}} = add [[POINTER]], 1
    function allowed(bytes memory s, uint128 set) public pure returns (bool) {
        for (uint256 i; i < s.length; ++i) {
            if ((set >> uint8(s[i])) & 1 == 0) return false;
        }
        return true;
    }
}
