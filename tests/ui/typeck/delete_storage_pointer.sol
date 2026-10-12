// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/213_no_delete_on_storage_pointers.sol

contract C {
    uint[] data;
    function f() public {
        uint[] storage x = data;
        delete x; //~ ERROR: cannot delete `uint256[] storage`
    }
}
