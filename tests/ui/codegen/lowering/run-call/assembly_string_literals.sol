//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: concat => 0x6162000000000000000000000000000000000000000000000000000000000000
//@ run-call: unicodeLit => 0xc3a9000000000000000000000000000000000000000000000000000000000000
//@ run-call: hexConcat => 0xabcd000000000000000000000000000000000000000000000000000000000000
//@ run-call: builtin => 0xabcd000000000000000000000000000000000000000000000000000000000000

// Assembly accepts adjacent string literals, `unicode` literals and adjacent `hex` literals; see
// PARSE-005 in docs/SOLC_DIVERGENCE.md.
contract AssemblyStringLiterals {
    // CHECK-LABEL: fn @concat()
    // CHECK: ret 0x6162000000000000000000000000000000000000000000000000000000000000
    function concat() public pure returns (uint256 r) {
        assembly {
            r := "a" "b"
        }
    }

    // CHECK-LABEL: fn @unicodeLit()
    // CHECK: ret 0xc3a9000000000000000000000000000000000000000000000000000000000000
    function unicodeLit() public pure returns (uint256 r) {
        assembly {
            r := unicode"é"
        }
    }

    // CHECK-LABEL: fn @hexConcat()
    // CHECK: ret 0xabcd000000000000000000000000000000000000000000000000000000000000
    function hexConcat() public pure returns (uint256 r) {
        assembly {
            r := hex"ab" hex"cd"
        }
    }

    // CHECK-LABEL: fn @builtin()
    // CHECK: ret 0xabcd000000000000000000000000000000000000000000000000000000000000
    function builtin() public pure returns (uint256 r) {
        assembly {
            r := or(hex"ab" hex"cd", 0)
        }
    }
}
