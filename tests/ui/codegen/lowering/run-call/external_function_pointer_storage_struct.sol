//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: read => 1, 2
// ported-from: test/libsolidity/semanticTests/functionTypes/struct_with_external_function.sol

contract ExternalFunctionPointerStorageStruct {
    struct Entry {
        uint16 first;
        function() external returns (uint256) callback;
        uint16 second;
    }

    Entry[2] entries;

    function firstTarget() external pure returns (uint256) {
        return 1;
    }

    function secondTarget() external pure returns (uint256) {
        return 2;
    }

    constructor() {
        entries[0].first = 0xff07;
        entries[0].second = 0xff07;
        entries[1].callback = this.secondTarget;
        entries[1].first = 0xff07;
        entries[1].second = 0xff07;
        entries[0].callback = this.firstTarget;
    }

    function read() external returns (uint256, uint256) {
        return (entries[0].callback(), entries[1].callback());
    }
}
