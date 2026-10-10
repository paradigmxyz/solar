//@ revisions: gas size none byzantium
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefixes=CHECK,GAS
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefixes=CHECK,SIZE
//@[none] compile-flags: -O none -Zdump=evm-ir-runtime
//@[none] filecheck: --check-prefixes=CHECK,NONE
//@[byzantium] compile-flags: -O gas --evm-version byzantium -Zdump=evm-ir-runtime
//@[byzantium] filecheck: --check-prefixes=CHECK,BYZ
//@ normalize-stdout-test: "(?s).+" -> ""

// Each contract holds one internal switch, so every switch gets the full
// gas-mode growth budget. The checks pin the shape that automatic selection
// picks for each case set.

contract SmallDense {
    // Four adjacent keys use a packed dense table in both modes. Byzantium
    // has no shifts to unpack it.
    // CHECK-LABEL: @module SmallDense_runtime
    // GAS: push 4{{$}}
    // GAS-NEXT: gt
    // GAS: indexed_jump {{(bb[0-9]+, ){3}bb[0-9]+$}}
    // SIZE: push 4{{$}}
    // SIZE-NEXT: gt
    // SIZE: indexed_jump {{(bb[0-9]+, ){3}bb[0-9]+$}}
    // BYZ-NOT: indexed_jump
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { return(0, 1) }
            case 1 { return(0, 2) }
            case 2 { return(0, 3) }
            case 3 { return(0, 4) }
            default { revert(0, 0) }
        }
    }
}

contract Conservative {
    // Gas mode pays for a table over 0..19. Size mode keeps the linear scan
    // because table entries need full-width labels before layout.
    // CHECK-LABEL: @module Conservative_runtime
    // GAS: push 20{{$}}
    // GAS-NEXT: gt
    // GAS: indexed_jump {{(bb[0-9]+, ){19}bb[0-9]+$}}
    // SIZE-NOT: indexed_jump
    // BYZ: push 20{{$}}
    // BYZ-NEXT: gt
    // BYZ: indexed_jump {{(bb[0-9]+, ){19}bb[0-9]+$}}
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { mstore(0, 100) }
            case 3 { mstore(0, 101) }
            case 5 { mstore(0, 102) }
            case 7 { mstore(0, 103) }
            case 8 { mstore(0, 104) }
            case 18 { mstore(0, 105) }
            case 19 { mstore(0, 106) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}

contract Affine5 {
    // An odd stride maps to a packed table by multiplying with its inverse.
    // CHECK-LABEL: @module Affine5_runtime
    // GAS: mul
    // GAS-NEXT: dup 1
    // GAS-NEXT: push 5{{$}}
    // GAS-NEXT: gt
    // GAS: indexed_jump {{(bb[0-9]+, ){4}bb[0-9]+$}}
    // SIZE-NOT: indexed_jump
    // BYZ-NOT: indexed_jump
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { mstore(0, 100) }
            case 7919 { mstore(0, 101) }
            case 15838 { mstore(0, 102) }
            case 23757 { mstore(0, 103) }
            case 31676 { mstore(0, 104) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}

contract Affine65 {
    // The affine table also takes more keys than the 64 that bit-slice and
    // bucket tables allow.
    // Without optimization the switch stays a linear scan.
    // CHECK-LABEL: @module Affine65_runtime
    // GAS: mul
    // GAS-NEXT: dup 1
    // GAS-NEXT: push 65{{$}}
    // GAS-NEXT: gt
    // GAS: indexed_jump
    // SIZE: mul
    // SIZE-NEXT: dup 1
    // SIZE-NEXT: push 65{{$}}
    // SIZE-NEXT: gt
    // SIZE: indexed_jump
    // NONE-NOT: {{^  (indexed_jump|mul|gt)$}}
    // BYZ: mul
    // BYZ-NEXT: dup 1
    // BYZ-NEXT: push 65{{$}}
    // BYZ-NEXT: gt
    // BYZ: indexed_jump
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { mstore(0, 100) }
            case 7919 { mstore(0, 101) }
            case 15838 { mstore(0, 102) }
            case 23757 { mstore(0, 103) }
            case 31676 { mstore(0, 104) }
            case 39595 { mstore(0, 105) }
            case 47514 { mstore(0, 106) }
            case 55433 { mstore(0, 107) }
            case 63352 { mstore(0, 108) }
            case 0x11667 { mstore(0, 109) }
            case 0x13556 { mstore(0, 110) }
            case 0x15445 { mstore(0, 111) }
            case 0x17334 { mstore(0, 112) }
            case 0x19223 { mstore(0, 113) }
            case 0x1b112 { mstore(0, 114) }
            case 0x1d001 { mstore(0, 115) }
            case 0x1eef0 { mstore(0, 116) }
            case 0x20ddf { mstore(0, 117) }
            case 0x22cce { mstore(0, 118) }
            case 0x24bbd { mstore(0, 119) }
            case 0x26aac { mstore(0, 120) }
            case 0x2899b { mstore(0, 121) }
            case 0x2a88a { mstore(0, 122) }
            case 0x2c779 { mstore(0, 123) }
            case 0x2e668 { mstore(0, 124) }
            case 0x30557 { mstore(0, 125) }
            case 0x32446 { mstore(0, 126) }
            case 0x34335 { mstore(0, 127) }
            case 0x36224 { mstore(0, 128) }
            case 0x38113 { mstore(0, 129) }
            case 0x3a002 { mstore(0, 130) }
            case 0x3bef1 { mstore(0, 131) }
            case 0x3dde0 { mstore(0, 132) }
            case 0x3fccf { mstore(0, 133) }
            case 0x41bbe { mstore(0, 134) }
            case 0x43aad { mstore(0, 135) }
            case 0x4599c { mstore(0, 136) }
            case 0x4788b { mstore(0, 137) }
            case 0x4977a { mstore(0, 138) }
            case 0x4b669 { mstore(0, 139) }
            case 0x4d558 { mstore(0, 140) }
            case 0x4f447 { mstore(0, 141) }
            case 0x51336 { mstore(0, 142) }
            case 0x53225 { mstore(0, 143) }
            case 0x55114 { mstore(0, 144) }
            case 0x57003 { mstore(0, 145) }
            case 0x58ef2 { mstore(0, 146) }
            case 0x5ade1 { mstore(0, 147) }
            case 0x5ccd0 { mstore(0, 148) }
            case 0x5ebbf { mstore(0, 149) }
            case 0x60aae { mstore(0, 150) }
            case 0x6299d { mstore(0, 151) }
            case 0x6488c { mstore(0, 152) }
            case 0x6677b { mstore(0, 153) }
            case 0x6866a { mstore(0, 154) }
            case 0x6a559 { mstore(0, 155) }
            case 0x6c448 { mstore(0, 156) }
            case 0x6e337 { mstore(0, 157) }
            case 0x70226 { mstore(0, 158) }
            case 0x72115 { mstore(0, 159) }
            case 0x74004 { mstore(0, 160) }
            case 0x75ef3 { mstore(0, 161) }
            case 0x77de2 { mstore(0, 162) }
            case 0x79cd1 { mstore(0, 163) }
            case 0x7bbc0 { mstore(0, 164) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}

contract BitSlice7 {
    // The low three bits separate every key. Each case block has one
    // predecessor, so it merges into the slot that checks its key.
    // CHECK-LABEL: @module BitSlice7_runtime
    // GAS: push 7{{$}}
    // GAS-NEXT: and
    // GAS-NEXT: indexed_jump {{(bb[0-9]+, ){7}bb[0-9]+$}}
    // SIZE-NOT: indexed_jump
    // BYZ: push 7{{$}}
    // BYZ-NEXT: and
    // BYZ-NEXT: indexed_jump {{(bb[0-9]+, ){7}bb[0-9]+$}}
    fallback() external {
        assembly {
            switch calldataload(0)
            case 1 { mstore(0, 100) }
            case 7920 { mstore(0, 101) }
            case 15839 { mstore(0, 102) }
            case 23758 { mstore(0, 103) }
            case 31677 { mstore(0, 104) }
            case 39596 { mstore(0, 105) }
            case 47515 { mstore(0, 106) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}

contract Coalesced16 {
    // Merged case guards make a 16-slot bit-slice table fit its growth cap.
    // Byzantium cannot shift, so it uses modulo buckets with one shared miss.
    // CHECK-LABEL: @module Coalesced16_runtime
    // GAS: push 15{{$}}
    // GAS-NEXT: and
    // GAS-NEXT: indexed_jump {{(bb[0-9]+, ){15}bb[0-9]+$}}
    // SIZE-NOT: indexed_jump
    // BYZ: push 17{{$}}
    // BYZ-NEXT: dup 2
    // BYZ-NEXT: mod
    // BYZ-NEXT: indexed_jump {{(bb[0-9]+, ){16}bb[0-9]+$}}
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0x10000 { mstore(0, 100) }
            case 0x40001 { mstore(0, 101) }
            case 0x90002 { mstore(0, 102) }
            case 0x100003 { mstore(0, 103) }
            case 0x190004 { mstore(0, 104) }
            case 0x240005 { mstore(0, 105) }
            case 0x310006 { mstore(0, 106) }
            case 0x400007 { mstore(0, 107) }
            case 0x510008 { mstore(0, 108) }
            case 0x640009 { mstore(0, 109) }
            case 0x79000a { mstore(0, 110) }
            case 0x90000b { mstore(0, 111) }
            case 0xa9000c { mstore(0, 112) }
            case 0xc4000d { mstore(0, 113) }
            case 0xe1000e { mstore(0, 114) }
            case 0x100000f { mstore(0, 115) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}

contract Binary32 {
    // Bucket tables would exceed the growth limit, so gas mode splits the
    // sorted keys into eight leaves of four.
    // CHECK-LABEL: @module Binary32_runtime
    // GAS-COUNT-7: {{^  gt$}}
    // GAS-NOT: {{^  (gt|indexed_jump)$}}
    // SIZE-NOT: indexed_jump
    // BYZ-COUNT-7: {{^  gt$}}
    // BYZ-NOT: {{^  (gt|indexed_jump)$}}
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { mstore(0, 100) }
            case 257 { mstore(0, 101) }
            case 1026 { mstore(0, 102) }
            case 2307 { mstore(0, 103) }
            case 4100 { mstore(0, 104) }
            case 6405 { mstore(0, 105) }
            case 9222 { mstore(0, 106) }
            case 12551 { mstore(0, 107) }
            case 16392 { mstore(0, 108) }
            case 20745 { mstore(0, 109) }
            case 25610 { mstore(0, 110) }
            case 30987 { mstore(0, 111) }
            case 36876 { mstore(0, 112) }
            case 43277 { mstore(0, 113) }
            case 50190 { mstore(0, 114) }
            case 57615 { mstore(0, 115) }
            case 0x10010 { mstore(0, 116) }
            case 0x12111 { mstore(0, 117) }
            case 0x14412 { mstore(0, 118) }
            case 0x16913 { mstore(0, 119) }
            case 0x19014 { mstore(0, 120) }
            case 0x1b915 { mstore(0, 121) }
            case 0x1e416 { mstore(0, 122) }
            case 0x21117 { mstore(0, 123) }
            case 0x24018 { mstore(0, 124) }
            case 0x27119 { mstore(0, 125) }
            case 0x2a41a { mstore(0, 126) }
            case 0x2d91b { mstore(0, 127) }
            case 0x3101c { mstore(0, 128) }
            case 0x3491d { mstore(0, 129) }
            case 0x3841e { mstore(0, 130) }
            case 0x3c11f { mstore(0, 131) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}

contract Rotated32 {
    // An even stride rotates out its trailing zero bit. Byzantium has no
    // shifts, so it falls back to a binary search.
    // CHECK-LABEL: @module Rotated32_runtime
    // GAS: mul
    // GAS: push 1{{$}}
    // GAS-NEXT: shr
    // GAS: push 255{{$}}
    // GAS-NEXT: shl
    // GAS-NEXT: or
    // SIZE: mul
    // SIZE: push 1{{$}}
    // SIZE-NEXT: shr
    // BYZ-NOT: {{^  (shr|shl|indexed_jump)$}}
    fallback() external {
        assembly {
            switch calldataload(0)
            case 1000 { mstore(0, 100) }
            case 1006 { mstore(0, 101) }
            case 1012 { mstore(0, 102) }
            case 1018 { mstore(0, 103) }
            case 1024 { mstore(0, 104) }
            case 1030 { mstore(0, 105) }
            case 1036 { mstore(0, 106) }
            case 1042 { mstore(0, 107) }
            case 1048 { mstore(0, 108) }
            case 1054 { mstore(0, 109) }
            case 1060 { mstore(0, 110) }
            case 1066 { mstore(0, 111) }
            case 1072 { mstore(0, 112) }
            case 1078 { mstore(0, 113) }
            case 1084 { mstore(0, 114) }
            case 1090 { mstore(0, 115) }
            case 1096 { mstore(0, 116) }
            case 1102 { mstore(0, 117) }
            case 1108 { mstore(0, 118) }
            case 1114 { mstore(0, 119) }
            case 1120 { mstore(0, 120) }
            case 1126 { mstore(0, 121) }
            case 1132 { mstore(0, 122) }
            case 1138 { mstore(0, 123) }
            case 1144 { mstore(0, 124) }
            case 1150 { mstore(0, 125) }
            case 1156 { mstore(0, 126) }
            case 1162 { mstore(0, 127) }
            case 1168 { mstore(0, 128) }
            case 1174 { mstore(0, 129) }
            case 1180 { mstore(0, 130) }
            case 1186 { mstore(0, 131) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}

contract Odd8 {
    // An odd stride needs no rotation, so Byzantium keeps the affine table.
    // CHECK-LABEL: @module Odd8_runtime
    // GAS: mul
    // GAS-NEXT: dup 1
    // GAS-NEXT: push 8{{$}}
    // GAS-NEXT: gt
    // GAS: indexed_jump {{(bb[0-9]+, ){7}bb[0-9]+$}}
    // BYZ: mul
    // BYZ-NEXT: dup 1
    // BYZ-NEXT: push 8{{$}}
    // BYZ-NEXT: gt
    // BYZ: indexed_jump {{(bb[0-9]+, ){7}bb[0-9]+$}}
    fallback() external {
        assembly {
            switch calldataload(0)
            case 0 { mstore(0, 100) }
            case 3 { mstore(0, 101) }
            case 6 { mstore(0, 102) }
            case 9 { mstore(0, 103) }
            case 12 { mstore(0, 104) }
            case 15 { mstore(0, 105) }
            case 18 { mstore(0, 106) }
            case 21 { mstore(0, 107) }
            default { mstore(0, 999) }
            return(0, 32)
        }
    }
}
