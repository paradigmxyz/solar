//@ run-call: test(uint8[][][]) [[[0], [7]], [[7], [8, 9]]]
//@ run-call: test2(uint8[][]) [[7], [8, 9]]
// ported-from: test/libsolidity/semanticTests/array/copying/nested_dynamic_array_element_calldata_to_storage.sol

contract StorageNestedDynamicCalldataToStorage {
    uint8[][] internal values;
    uint8[][][] internal nested_values;

    function test(uint8[][][] calldata input) public {
        values = input[1];
        require(values.length == 2);
        require(values[0].length == 1);
        require(values[0][0] == 7);
        require(values[1].length == 2);
        require(values[1][0] == 8);
        require(values[1][1] == 9);
    }

    function test2(uint8[][] calldata input) public {
        nested_values = new uint8[][][](2);
        nested_values[0] = input;
        require(nested_values[0].length == 2);
        require(nested_values[0][0].length == 1);
        require(nested_values[0][0][0] == 7);
        require(nested_values[0][1].length == 2);
        require(nested_values[0][1][0] == 8);
        require(nested_values[0][1][1] == 9);
        require(nested_values[1].length == 0);
    }
}
