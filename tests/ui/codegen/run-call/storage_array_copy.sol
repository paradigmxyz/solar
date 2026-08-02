//@ run-call: copyState => 1, 3
//@ run-call: copyReference => 4, 2
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
