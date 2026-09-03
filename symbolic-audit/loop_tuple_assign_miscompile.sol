// Finding 36: at -Ogas, a loop whose body assigns a tuple returned by an internal function,
// after a branch that assigns one of the same variables, returns a wrong value. -Onone and
// -Osize agree with solc; the EVM IR pipeline is not involved (-Zevm-ir-pipeline=none still
// differs). Reduced from symbolic-audit/probes/stack_pressure.sol f2 and f12.
//   python3 symbolic-audit/tools/statediff.py symbolic-audit/loop_tuple_assign_miscompile.sol R \
//     --fixed "f2(uint256,uint256,uint256) 0 1 0" --fixed "f2(uint256,uint256,uint256) 3 5 7"
//   python3 symbolic-audit/tools/statediff.py symbolic-audit/loop_tuple_assign_miscompile.sol R --no-optimize \
//     --fixed "f2(uint256,uint256,uint256) 0 1 0"
contract R {
    function h(uint256 x, uint256 y) internal pure returns (uint256, uint256) { unchecked { return (x * 3 + y, x ^ (y << 1)); } }
    function k(uint256 x) internal pure returns (uint256) { unchecked { return x * 0x9e3779b97f4a7c15 + 1; } }
    function f2(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1; uint256 v1 = c + 2; uint256 v2 = b + 3;
            uint256 v3 = (v1 - v2); uint256 v4 = (v0 + v1); uint256 v5 = (v1 - v2); uint256 v6 = (v5 ^ v1); uint256 v7 = (v3 + v6);
            if (v2 & 1 == 1) { v7 = (v6 * v4); } else { v2 = k(v6); }
            for (uint256 i = 0; i < (v4 & 3); i++) { (v3, v2) = h(v1, v2); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }
    // The same shape with the tuple written inline instead of through `h` also differs.
    function f2Inline(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        unchecked {
            uint256 v0 = a + 1; uint256 v1 = c + 2; uint256 v2 = b + 3;
            uint256 v3 = (v1 - v2); uint256 v4 = (v0 + v1); uint256 v5 = (v1 - v2); uint256 v6 = (v5 ^ v1); uint256 v7 = (v3 + v6);
            if (v2 & 1 == 1) { v7 = (v6 * v4); } else { v2 = k(v6); }
            for (uint256 i = 0; i < (v4 & 3); i++) { (v3, v2) = (v1 * 3 + v2, v1 ^ (v2 << 1)); }
            return (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8);
        }
    }
}
