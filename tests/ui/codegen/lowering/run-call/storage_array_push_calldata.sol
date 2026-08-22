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
//@[none, gas, size] run-call: pushArray(uint120[]) [1, 2, 3] => 1
//@[none, gas, size] run-call: pushStruct((uint16,uint16,uint16[3],uint16[])) (2, 3, [0, 0, 4], [0, 0, 5, 0]) => 2345
// ported-from: test/libsolidity/semanticTests/array/push/array_push_nested_from_calldata.sol
// ported-from: test/libsolidity/semanticTests/array/push/array_push_struct_from_calldata.sol

pragma abicoder v2;

contract StorageArrayPushCalldata {
    uint120[][] private nested;

    struct Entry {
        uint16 a;
        uint16 b;
        uint16[3] c;
        uint16[] d;
    }

    Entry[] private entries;

    function pushArray(uint120[] calldata values) external returns (uint120) {
        nested.push(values);
        return nested[0][0];
    }

    function pushStruct(Entry calldata entry) external returns (uint256) {
        entries.push(entry);
        return uint256(entries[0].a) * 1000 + uint256(entries[0].b) * 100
            + uint256(entries[0].c[2]) * 10 + entries[0].d[2];
    }
}
