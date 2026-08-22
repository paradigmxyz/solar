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
//@[none] run-call: Creator::f() => 23
//@[gas] run-call: Creator::f() => 23
//@[size] run-call: Creator::f() => 23

contract ConstructorAggregateBase {
    struct Entry {
        uint256 value;
        bytes data;
    }

    Entry[] internal entries;

    constructor(Entry[] memory input) {
        entries = input;
    }
}

contract ConstructorAggregateMain is ConstructorAggregateBase {
    constructor(Entry[] memory input) ConstructorAggregateBase(input) {}

    function read() external view returns (uint256) {
        return entries[0].value + entries[0].data.length + entries[1].value
            + entries[1].data.length;
    }
}

contract Creator {
    function f() external returns (uint256) {
        ConstructorAggregateBase.Entry[] memory input =
            new ConstructorAggregateBase.Entry[](2);
        input[0].value = 7;
        input[0].data = hex"0102";
        input[1].value = 11;
        input[1].data = hex"030405";
        ConstructorAggregateMain created = new ConstructorAggregateMain(input);
        return created.read();
    }
}
