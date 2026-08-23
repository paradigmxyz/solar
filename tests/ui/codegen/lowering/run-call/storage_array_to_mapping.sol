//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: from_storage() => [[10, 11], [12, 13, 14]]
//@ run-call: from_storage_ptr() => [[10, 11], [12, 13, 14]]
//@ run-call: from_memory() => [[10, 11], [12, 13, 14]]
//@ run-call: from_calldata(uint8[][]) [[10, 11], [12, 13, 14]] => [[10, 11], [12, 13, 14]]
// ported-from: test/libsolidity/semanticTests/array/copying/array_to_mapping.sol
// ported-from: test/libsolidity/semanticTests/array/copying/calldata_array_to_mapping.sol

contract StorageArrayToMapping {
    mapping(uint256 => uint8[][]) internal mapped;
    uint8[][] internal source;

    constructor() {
        source = new uint8[][](2);

        source[0] = new uint8[](2);
        source[0][0] = 10;
        source[0][1] = 11;

        source[1] = new uint8[](3);
        source[1][0] = 12;
        source[1][1] = 13;
        source[1][2] = 14;
    }

    function from_storage() public returns (uint8[][] memory) {
        mapped[0] = source;
        return mapped[0];
    }

    function from_storage_ptr() public returns (uint8[][] memory) {
        uint8[][] storage source_ptr = source;
        mapped[0] = source_ptr;
        return mapped[0];
    }

    function from_memory() public returns (uint8[][] memory) {
        uint8[][] memory copied = source;
        mapped[0] = copied;
        return mapped[0];
    }

    function from_calldata(uint8[][] calldata input) public returns (uint8[][] memory) {
        mapped[0] = input;
        return mapped[0];
    }
}
