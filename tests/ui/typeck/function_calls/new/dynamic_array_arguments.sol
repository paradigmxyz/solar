//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/invalidArgs/creating_memory_array.sol

contract C {
    function f(uint256 n) public {
        uint256[] memory y = new uint256[](); //~ ERROR: wrong argument count

        bytes memory b = new bytes(n);
        string memory s = new string(n);
        address payable[] memory a = new address payable[](10);
    }
}
