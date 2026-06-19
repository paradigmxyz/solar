//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/array/bytes1_array_push_assign_multi.sol

contract Test {
    bytes1[] byteArray;
    bytes1[] otherByteArray;

    function tuplePushLvalues() external {
        (byteArray.push(), byteArray.push()) = (bytes1(0), bytes1(0));
        (((byteArray.push())), (byteArray.push())) = (bytes1(0), bytes1(0));
        ((byteArray.push(), byteArray.push()), byteArray.push()) =
            ((bytes1(0), bytes1(0)), bytes1(0));
        (byteArray.push(), byteArray[0]) = (bytes1(0), bytes1(0));
        bytes1[] storage local = byteArray;
        (byteArray.push(), local.push()) = (bytes1(0), bytes1(0));
        (byteArray.push(), otherByteArray.push()) = (bytes1(0), bytes1(0));
    }
}
