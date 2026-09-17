//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ run-call: low 5 => 20
//@ run-call: low 0 => 0
//@ run-call: high 5 => 35
//@ run-call: high 0 => 0
//@ run-call: shared 5 => 55

// Independent external entries may overlay their frames; a helper reached by both entries
// must reserve space above both callers. Distinct loop bodies keep the calls out of tiny-leaf
// inlining. High-memory assembly in one entry must not raise unrelated helpers' heap floors.
contract StaticFrameContext {
    function low(uint256 x) external pure returns (uint256) {
        return twice(x);
    }
    function high(uint256 x) external pure returns (uint256) {
        assembly { mstore(0x1fe0, x) }
        uint256 result = fourTimes(x);
        assembly { result := add(result, mload(0x1fe0)) }
        return result;
    }
    function shared(uint256 x) external pure returns (uint256) {
        return twice(x) + fourTimes(x) + x;
    }
    function twice(uint256 x) internal pure returns (uint256 r) {
        for (uint256 i; i < x; ++i) { r += 2; }
        return r + sum(x);
    }
    function fourTimes(uint256 x) internal pure returns (uint256 r) {
        for (uint256 i; i < x; ++i) { r += 4; }
        return r + sum(x);
    }
    function sum(uint256 x) internal pure returns (uint256 r) {
        for (uint256 i; i < x; ++i) { r += i; }
    }
}
