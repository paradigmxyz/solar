//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: exercise() => 3, 9, 1, 8, 1, 3, 3, 7, 0, 3

contract StorageCopyRecursiveStruct {
    struct Node {
        uint128 value;
        bytes payload;
        Node[] children;
    }

    Node private source;
    Node private empty;
    Node[] private copies;

    function exercise()
        external
        returns (uint256, uint128, uint256, uint128, uint256, uint128, uint256, uint128, uint256, uint256)
    {
        source.value = 1;
        source.payload = hex"010203";
        source.children.push();
        source.children[0].value = 2;
        source.children[0].children.push();
        source.children[0].children[0].value = 3;
        source.children.push();
        source.children[1].value = 4;

        copies.push(source);

        source.value = 9;
        source.children.pop();
        source.children[0].value = 7;
        copies.push(source);

        source.children[0].value = 8;
        copies[0] = source;
        copies.push(empty);

        return (
            copies.length,
            copies[0].value,
            copies[0].children.length,
            copies[0].children[0].value,
            copies[0].children[0].children.length,
            copies[0].children[0].children[0].value,
            copies[0].payload.length,
            copies[1].children[0].value,
            copies[2].children.length,
            copies[1].payload.length
        );
    }
}
