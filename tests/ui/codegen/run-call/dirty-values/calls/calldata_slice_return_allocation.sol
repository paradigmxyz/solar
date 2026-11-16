//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: t 0x0102 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: viaPointer 0x0102 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

// A calldata slice returned by an internal call, directly or through a function pointer, keeps
// the length assembly gave it, so copying it to memory fails the allocation check before any
// calldata range check, as in solc via-IR.

contract C {
    function pick(bytes calldata d) internal pure returns (bytes calldata r) {
        r = d;
        assembly { r.length := not(0) }
    }
    function t(bytes calldata d) external pure returns (uint256) {
        bytes memory m = pick(d);
        return m.length;
    }

    function viaPointer(bytes calldata d) external pure returns (uint256) {
        function(bytes calldata) internal pure returns (bytes calldata) f = pick;
        bytes memory m = f(d);
        return m.length;
    }
}
