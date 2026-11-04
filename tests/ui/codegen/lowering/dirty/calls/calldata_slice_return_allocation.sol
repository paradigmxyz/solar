//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: t 0x0102 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

// A calldata slice returned by an internal call keeps the length assembly gave it, so copying it
// to memory fails the allocation check, as in solc via-IR, before any calldata range check.

contract C {
    function pick(bytes calldata d) internal pure returns (bytes calldata r) {
        r = d;
        assembly { r.length := not(0) }
    }
    function t(bytes calldata d) external pure returns (uint256) {
        bytes memory m = pick(d);
        return m.length;
    }
}
