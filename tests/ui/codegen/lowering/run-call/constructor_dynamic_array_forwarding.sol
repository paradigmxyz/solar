//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: Creator::f(uint256,address[]) 7, [0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 0x0000000000000000000000000000000000000003, 0x0000000000000000000000000000000000000004, 0x0000000000000000000000000000000000000005, 0x0000000000000000000000000000000000000006, 0x0000000000000000000000000000000000000007, 0x0000000000000000000000000000000000000008, 0x0000000000000000000000000000000000000009, 0x000000000000000000000000000000000000000a] => 7, 0x0000000000000000000000000000000000000008
// ported-from: test/libsolidity/semanticTests/constructor/arrays_in_constructors.sol

contract Base {
    uint256 public value;
    address[] private entries;

    constructor(uint256 value_, address[] memory entries_) {
        value = value_;
        entries = entries_;
    }

    function entry(uint256 index) external view returns (address) {
        return entries[index];
    }
}

contract Main is Base {
    constructor(address[] memory entries_, uint256 value_) Base(value_, forward(entries_)) {}

    function forward(address[] memory entries_) internal pure returns (address[] memory) {
        return entries_;
    }
}

contract Creator {
    function f(uint256 index, address[] memory entries)
        external
        returns (uint256 value, address selected)
    {
        Main created = new Main(entries, index);
        value = created.value();
        selected = created.entry(index);
    }
}
