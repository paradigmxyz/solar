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
//@[none, gas, size] run-call: pushEmpty => 0
//@[none, gas, size] run-call: pushMemory => 1
//@[none, gas, size] run-call: pushStruct => 2345
//@[none, gas, size] run-call: pushBytes => 97
// ported-from: test/libsolidity/semanticTests/array/push/array_push_nested.sol
// ported-from: test/libsolidity/semanticTests/array/push/array_push_nested_from_memory.sol
// ported-from: test/libsolidity/semanticTests/array/push/array_push_struct.sol
// ported-from: test/libsolidity/semanticTests/array/push/nested_bytes_push.sol

pragma abicoder v2;

contract StorageArrayPushNested {
    uint120[][] private nested;

    struct Entry {
        uint16 a;
        uint16 b;
        uint16[3] c;
        uint16[] d;
    }

    Entry[] private entries;
    bytes[] private byteValues;

    function pushEmpty() external returns (uint256) {
        nested.push();
        nested[0].push();
        return nested[0][0];
    }

    function pushMemory() external returns (uint256) {
        uint120[] memory values = new uint120[](3);
        values[0] = 1;
        nested.push(values);
        return nested[0][0];
    }

    function pushStruct() external returns (uint256) {
        Entry memory entry;
        entry.a = 2;
        entry.b = 3;
        entry.c[2] = 4;
        entry.d = new uint16[](4);
        entry.d[2] = 5;
        entries.push(entry);
        return uint256(entries[0].a) * 1000 + uint256(entries[0].b) * 100
            + uint256(entries[0].c[2]) * 10 + entries[0].d[2];
    }

    function pushBytes() external returns (uint256) {
        byteValues.push("abc");
        byteValues.push("abcdefghabcdefghabcdefghabcdefgh");
        byteValues.push("abcdefghabcdefghabcdefghabcdefghabcdefghabcdefghabcdefghabcdefgh");
        return uint8(byteValues[0][0]);
    }
}
