//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: test_nested() => 2, 0x68656c6c6f
//@ run-call: test_strings() => 0x776f726c64

contract IndexedAggregates {
    struct Payload {
        uint a;
        uint8[][] nested;
        bytes tail;
    }

    event Nested(uint8[][] indexed values, bytes indexed tail);
    event Strings(string indexed a, string[] indexed many);

    function test_nested() external returns (uint, bytes memory) {
        uint8[][] memory nested = new uint8[][](2);
        nested[0] = new uint8[](2);
        nested[0][0] = 1;
        nested[0][1] = 2;
        nested[1] = new uint8[](0);
        emit Nested(nested, "hello");
        return (nested.length, "hello");
    }

    function test_strings() external returns (bytes memory) {
        string[] memory many = new string[](1);
        many[0] = "x";
        emit Strings("world", many);
        return "world";
    }
}
