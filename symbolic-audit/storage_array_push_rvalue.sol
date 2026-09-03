// Finding 37: `a.push()` used as a value on a non-bytes storage array returns 0 instead of the
// appended element. Neither compiler clears the new slot, so the difference shows when the slot
// holds a value written through assembly. The `bytes` case was fixed in 3d5d21f8.
//   python3 symbolic-audit/tools/statediff.py symbolic-audit/storage_array_push_rvalue.sol A \
//     --fixed "dirtyThenPush(uint256) 0xff" --fixed "dirtyThenPush8(uint256) 0x7f" --fixed "pushClean()"
contract A {
    uint256[] a;
    uint8[] p;
    // Write `v` into the slot the next push will use, then push and return what push() yields.
    function dirtyThenPush(uint256 v) external returns (uint256 r) {
        uint256 len = a.length;
        assembly { mstore(0, a.slot) sstore(add(keccak256(0, 32), len), v) }
        r = a.push();
    }
    function dirtyThenPush8(uint256 v) external returns (uint8 r) {
        uint256 len = p.length;
        assembly { mstore(0, p.slot) sstore(add(keccak256(0, 32), div(len, 32)), v) }
        r = p.push();
    }
    function pushClean() external returns (uint256) { return a.push(); }
    function len() external view returns (uint256, uint256) { return (a.length, p.length); }
}
