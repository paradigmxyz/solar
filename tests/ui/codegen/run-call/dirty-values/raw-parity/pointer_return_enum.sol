//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: skipped => 7
//@ run-call-fail: read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: rawLocal => 0x101

// A function pointer that assembly retyped to return an enum or a `uint8` returns the word its
// target gave. The local that receives it keeps the word as it is, so a path that never reads the
// enum does not fail, a read of it panics with 0x21, and assembly sees the bits above a `uint8`,
// as in solc via-IR.

enum E { A, B }

contract C {
    function five() internal pure returns (uint256) {
        return 5;
    }

    function wide() internal pure returns (uint256) {
        return 0x101;
    }

    function skipped() external pure returns (uint256) {
        function() internal pure returns (uint256) p = five;
        function() internal pure returns (E) q;
        assembly { q := p }
        E x = q();
        if (true) return 7;
        return uint256(uint8(x));
    }

    function read() external pure returns (uint256) {
        function() internal pure returns (uint256) p = five;
        function() internal pure returns (E) q;
        assembly { q := p }
        E x = q();
        return uint256(uint8(x));
    }

    function rawLocal() external pure returns (uint256 r) {
        function() internal pure returns (uint256) p = wide;
        function() internal pure returns (uint8) q;
        assembly { q := p }
        uint8 x = q();
        assembly { r := x }
    }
}
