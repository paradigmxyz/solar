//@ compile-flags: -Osize --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Calls to either of two identical internal functions land on one entry `JUMPDEST`, so it keeps
// the origin of both definitions, which the source map writes as `-1` and ethdebug as a `pick`.

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: [invoke={{[pq]}}@{{[0-9]+..[0-9]+}}{{.*}}] !metadata(spans=[[[P:[0-9]+..[0-9]+]], [[Q:[0-9]+..[0-9]+]]])
contract C {
    function p(uint256 x) internal pure returns (uint256) {
        return x * 3 + 1;
    }

    function q(uint256 x) internal pure returns (uint256) {
        return x * 3 + 1;
    }

    function f(uint256 a) external pure returns (uint256) {
        return p(a + 2) + q(a + 3);
    }
}
