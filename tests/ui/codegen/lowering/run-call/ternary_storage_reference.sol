//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: choose(bool) true => 7, 0
//@[gas] run-call: choose(bool) true => 7, 0
//@[size] run-call: choose(bool) true => 7, 0
//@[none] run-call: choose(bool) false => 0, 7
//@[gas] run-call: choose(bool) false => 0, 7
//@[size] run-call: choose(bool) false => 0, 7
//@[none] run-call: chooseArray(bool) true => 11
//@[gas] run-call: chooseArray(bool) true => 11
//@[size] run-call: chooseArray(bool) true => 11
//@[none] run-call: chooseArray(bool) false => 22
//@[gas] run-call: chooseArray(bool) false => 22
//@[size] run-call: chooseArray(bool) false => 22

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
