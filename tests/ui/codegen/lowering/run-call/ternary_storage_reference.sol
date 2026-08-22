//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: choose(bool) true => 7, 0
//@[none, gas, size] run-call: choose(bool) false => 0, 7
//@[none, gas, size] run-call: chooseArray(bool) true => 11
//@[none, gas, size] run-call: chooseArray(bool) false => 22

contract TernaryStorageReference {
    struct S {
        uint256 x;
    }

    S a;
    S b;
    uint256[] first;
    uint256[] second;

    constructor() {
        first.push(11);
        second.push(22);
    }

    function choose(bool condition) external returns (uint256, uint256) {
        S storage slot = a;
        condition ? slot = a : slot = b;
        slot.x = 7;
        return (a.x, b.x);
    }

    function chooseArray(bool condition) external view returns (uint256) {
        uint256[] storage slot;
        slot = condition ? first : second;
        return slot[0];
    }
}
