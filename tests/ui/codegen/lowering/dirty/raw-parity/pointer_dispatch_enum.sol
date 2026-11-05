//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: valid => 1
//@ run-call-fail: outOfRange => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: unused => 7
//@ run-call: unusedBranch => 9
//@ run-call-fail: dirtyHigh => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

// A function pointer that assembly retyped to take a word passes it to an enum parameter. The
// callee range-checks the whole word where it reads the enum, as solc via-IR does, instead of
// masking it to the enum's width, and a callee that does not read it never fails.

enum E { A, B }

contract C {
    function f(E x) internal pure returns (uint256) {
        return uint256(uint8(x));
    }

    function ignore(E) internal pure returns (uint256) {
        return 7;
    }

    function branch(E x, bool skip) internal pure returns (uint256) {
        if (skip) return 9;
        return uint256(uint8(x));
    }

    function call(uint256 word) internal pure returns (uint256) {
        function(E) internal pure returns (uint256) p = f;
        function(uint256) internal pure returns (uint256) q;
        assembly { q := p }
        return q(word);
    }

    function valid() external pure returns (uint256) {
        return call(1);
    }

    function outOfRange() external pure returns (uint256) {
        return call(5);
    }

    function dirtyHigh() external pure returns (uint256) {
        return call(0xab00000000000000000000000000000000000000000000000000000000000001);
    }

    function unused() external pure returns (uint256) {
        function(E) internal pure returns (uint256) p = ignore;
        function(uint256) internal pure returns (uint256) q;
        assembly { q := p }
        return q(5);
    }

    function unusedBranch() external pure returns (uint256) {
        function(E, bool) internal pure returns (uint256) p = branch;
        function(uint256, bool) internal pure returns (uint256) q;
        assembly { q := p }
        return q(5, true);
    }
}
