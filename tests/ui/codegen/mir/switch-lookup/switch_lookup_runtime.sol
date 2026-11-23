//@ codegen-matrix: standard
//@ run-call: clOrder 0 => 16
//@ run-call: clOrder 1 => 17
//@ run-call: clOrder 2 => 18
//@ run-call: clOrder 3 => 0
//@ run-call: clOrder 4 => 8
//@ run-call: clOrder 5 => 7
//@ run-call: clOrder 6 => 9
//@ run-call: clOrder 7 => 6
//@ run-call: clOrder 8 => 10
//@ run-call: clOrder 9 => 5
//@ run-call: clOrder 10 => 11
//@ run-call: clOrder 11 => 4
//@ run-call: clOrder 12 => 12
//@ run-call: clOrder 13 => 3
//@ run-call: clOrder 14 => 13
//@ run-call: clOrder 15 => 2
//@ run-call: clOrder 16 => 14
//@ run-call: clOrder 17 => 1
//@ run-call: clOrder 18 => 15
//@ run-call: clOrder 19 => 0
//@ run-call: clOrder 32 => 0
//@ run-call: clOrder 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: clOrder 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff3 => 0
//@ run-call: distBase 0 => 1
//@ run-call: distBase 1 => 2
//@ run-call: distBase 2 => 3
//@ run-call: distBase 3 => 4
//@ run-call: distBase 4 => 5
//@ run-call: distBase 5 => 7
//@ run-call: distBase 6 => 9
//@ run-call: distBase 7 => 13
//@ run-call: distBase 8 => 17
//@ run-call: distBase 9 => 25
//@ run-call: distBase 10 => 33
//@ run-call: distBase 11 => 49
//@ run-call: distBase 12 => 65
//@ run-call: distBase 13 => 97
//@ run-call: distBase 14 => 129
//@ run-call: distBase 15 => 193
//@ run-call: distBase 16 => 257
//@ run-call: distBase 17 => 385
//@ run-call: distBase 18 => 513
//@ run-call: distBase 19 => 769
//@ run-call: distBase 20 => 1025
//@ run-call: distBase 21 => 1537
//@ run-call: distBase 22 => 2049
//@ run-call: distBase 23 => 3073
//@ run-call: distBase 24 => 4097
//@ run-call: distBase 25 => 6145
//@ run-call: distBase 26 => 8193
//@ run-call: distBase 27 => 12289
//@ run-call: distBase 28 => 16385
//@ run-call: distBase 29 => 24577
//@ run-call: distBase 30 => 0
//@ run-call: distBase 0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeef1 => 0
//@ run-call: distBase 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: lengths 3 => 100, 1
//@ run-call: lengths 4 => 7, 0
//@ run-call: lengths 5 => 200, 2
//@ run-call: lengths 6 => 7, 0
//@ run-call: lengths 7 => 7, 0
//@ run-call: lengths 8 => 7, 0
//@ run-call: lengths 9 => 300, 3
//@ run-call: lengths 2 => 7, 0
//@ run-call: lengths 10 => 7, 0
//@ run-call: lengths 0 => 7, 0
//@ run-call: lengths 0x8ba2e8ba2e8ba2e8ba2e8ba2e8ba2e8ba2e8ba2e8ba2e8ba2e8ba2e8ba2e8ba6 => 7, 0
//@ run-call: lengths 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 7, 0
//@ run-call: computed 0 => 5
//@ run-call: computed 1 => 6
//@ run-call: computed 3 => 8
//@ run-call: computed 2 => 1002
//@ run-call: computed 4 => 1004
//@ run-call: computed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 999
//@ run-call: lowKeys 10 => 3
//@ run-call: lowKeys 11 => 1
//@ run-call: lowKeys 12 => 4
//@ run-call: lowKeys 13 => 1
//@ run-call: lowKeys 9 => 99
//@ run-call: lowKeys 0 => 99
//@ run-call: lowKeys 14 => 99
//@ run-call: lowKeys 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffee => 99
//@ run-call: lowKeys 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffed => 99
//@ run-call: lowKeys 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 99
//@ run-call: highKeys 40 => 2
//@ run-call: highKeys 41 => 7
//@ run-call: highKeys 42 => 1
//@ run-call: highKeys 43 => 8
//@ run-call: highKeys 39 => 0
//@ run-call: highKeys 44 => 0
//@ run-call: highKeys 0 => 0
//@ run-call: highKeys 11 => 0
//@ run-call: highKeys 12 => 0
//@ run-call: highKeys 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0

// Yul switches whose cases only assign literals become packed table lookups: a byte table
// (`clOrder`), two words of 15-bit slots (`distBase`), two outputs in one slot with gaps and
// nonzero defaults (`lengths`), a computed default that stays a branch (`computed`), and byte
// tables of keys from 10, read at `v + 18`, and from 40, read at `v - 12` (`lowKeys`, `highKeys`).
// The calls cover every key, the gaps, values past the keys, and values whose slot shift or byte
// index wraps.
contract SwitchLookupRuntime {
    function clOrder(uint256 i) external pure returns (uint256 o) {
        assembly {
            switch i
            case 0 { o := 16 }
            case 1 { o := 17 }
            case 2 { o := 18 }
            case 3 { o := 0 }
            case 4 { o := 8 }
            case 5 { o := 7 }
            case 6 { o := 9 }
            case 7 { o := 6 }
            case 8 { o := 10 }
            case 9 { o := 5 }
            case 10 { o := 11 }
            case 11 { o := 4 }
            case 12 { o := 12 }
            case 13 { o := 3 }
            case 14 { o := 13 }
            case 15 { o := 2 }
            case 16 { o := 14 }
            case 17 { o := 1 }
            case 18 { o := 15 }
            default { o := 0 }
        }
    }

    function distBase(uint256 c) external pure returns (uint256 b) {
        assembly {
            switch c
            case 0 { b := 1 }
            case 1 { b := 2 }
            case 2 { b := 3 }
            case 3 { b := 4 }
            case 4 { b := 5 }
            case 5 { b := 7 }
            case 6 { b := 9 }
            case 7 { b := 13 }
            case 8 { b := 17 }
            case 9 { b := 25 }
            case 10 { b := 33 }
            case 11 { b := 49 }
            case 12 { b := 65 }
            case 13 { b := 97 }
            case 14 { b := 129 }
            case 15 { b := 193 }
            case 16 { b := 257 }
            case 17 { b := 385 }
            case 18 { b := 513 }
            case 19 { b := 769 }
            case 20 { b := 1025 }
            case 21 { b := 1537 }
            case 22 { b := 2049 }
            case 23 { b := 3073 }
            case 24 { b := 4097 }
            case 25 { b := 6145 }
            case 26 { b := 8193 }
            case 27 { b := 12289 }
            case 28 { b := 16385 }
            case 29 { b := 24577 }
            default { b := 0 }
        }
    }

    function lengths(uint256 c) external pure returns (uint256 b, uint256 e) {
        assembly {
            switch c
            case 3 { b := 100 e := 1 }
            case 5 { b := 200 e := 2 }
            case 9 { b := 300 e := 3 }
            default { b := 7 e := 0 }
        }
    }

    function lowKeys(uint256 i) external pure returns (uint256 o) {
        assembly {
            switch i
            case 10 { o := 3 }
            case 11 { o := 1 }
            case 12 { o := 4 }
            case 13 { o := 1 }
            default { o := 99 }
        }
    }

    function highKeys(uint256 i) external pure returns (uint256 o) {
        assembly {
            switch i
            case 40 { o := 2 }
            case 41 { o := 7 }
            case 42 { o := 1 }
            case 43 { o := 8 }
            default { o := 0 }
        }
    }

    function computed(uint256 i) external pure returns (uint256 o) {
        assembly {
            switch i
            case 0 { o := 5 }
            case 1 { o := 6 }
            case 3 { o := 8 }
            default { o := add(i, 1000) }
        }
    }
}
