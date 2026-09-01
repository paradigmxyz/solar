//@ codegen-matrix: standard
//@ run-call: DeleteStorageDynamicArray::clearAndPush => 1, 9

contract DeleteStorageDynamicArray {
    uint24[] internal values;

    function clearAndPush() external returns (uint256 len, uint24 first) {
        values.push(7);
        assembly {
            mstore(0, 32)
        }
        delete values;
        values.push(9);
        return (values.length, values[0]);
    }
}
