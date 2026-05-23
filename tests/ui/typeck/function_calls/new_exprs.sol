//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionCalls/new_library.sol
// ported-from: test/libsolidity/syntaxTests/array/library_array.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/523_reject_interface_creation.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/029_create_abstract_contract.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/265_new_for_non_array.sol
// ported-from: test/libsolidity/syntaxTests/array/new_no_parentheses.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/invalidArgs/creating_memory_array.sol

library L {}

interface I {}

abstract contract A {
    function a() public virtual;
}

contract ValidContract {}

contract C {
    function f(uint256 n) public {
        new L(); //~ ERROR: cannot instantiate librarys
        new L[](2); //~ ERROR: invalid use of a library name
        new I(); //~ ERROR: cannot instantiate interfaces
        new A(); //~ ERROR: cannot instantiate abstract contracts

        uint256 x = new uint256(7); //~ ERROR: expected contract or dynamic array type

        new uint256[1]; //~ ERROR: cannot instantiate static arrays

        uint256[] memory y = new uint256[](); //~ ERROR: wrong argument count

        bytes memory b = new bytes(n);
        string memory s = new string(n);
        address payable[] memory a = new address payable[](10);
        ValidContract[] memory contracts = new ValidContract[](1);
    }
}
