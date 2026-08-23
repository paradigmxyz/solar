//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: copyState => 1, 3
//@ run-call: copyReference => 4, 2
//@ run-call: copyAggregate => 7, 9, 1
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_storage_to_memory.sol

contract C {
    uint[] a;

    function copyState() public returns (uint, uint) {
        a.push(1);
        a.push(0);
        a.push(0);
        uint[] memory b = a;
        return (b[0], b.length);
    }

    function copyReference() public returns (uint, uint) {
        a.push(4);
        a.push(0);
        uint[] storage r = a;
        uint[] memory b = r;
        return (b[0], b.length);
    }
}

contract AggregateStorageCopy {
    struct Pair {
        uint x;
        uint y;
    }

    Pair[] pairs;

    function copyAggregate() public returns (uint, uint, uint) {
        pairs.push();
        pairs[0].x = 7;
        pairs[0].y = 9;
        Pair[] memory copied = pairs;
        return (copied[0].x, copied[0].y, copied.length);
    }
}
