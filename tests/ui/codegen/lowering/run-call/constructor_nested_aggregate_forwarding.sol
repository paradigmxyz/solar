//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: Creator::f => 23

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
