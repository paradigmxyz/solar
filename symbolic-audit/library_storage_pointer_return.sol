// Finding 42: an external library function that returns a storage pointer is miscompiled from
// byzantium on. The library's ABI wrapper encodes the return as a memory array instead of the
// slot word (solc: storage pointers travel as uint256 slot numbers), so the caller receives a
// wrong slot: `.length` through the pointer reads 0 where solc reads 2, and a push through it
// writes another slot. Pre-byzantium the call is rejected instead. The stateful harness cannot
// link libraries; verify with the linked-library runner:
//   python3 symbolic-audit/tools/libdiff.py symbolic-audit/library_storage_pointer_return.sol L C --fixed "len()" --fixed "sum()" --fixed "pushThrough(uint256) 3" --fixed "len()"
library L {
    function arrRef(uint256[] storage a) external pure returns (uint256[] storage) { return a; }
}
contract C {
    uint256[] internal nums;
    uint256 internal guard;
    constructor() { nums.push(5); nums.push(9); guard = 77; }
    function len() external returns (uint256) { return L.arrRef(nums).length; }
    function sum() external returns (uint256 s) {
        uint256[] storage r = L.arrRef(nums);
        for (uint256 i; i < r.length; i++) s += r[i];
    }
    function pushThrough(uint256 v) external returns (uint256, uint256) {
        L.arrRef(nums).push(v);
        return (nums.length, guard);
    }
}
