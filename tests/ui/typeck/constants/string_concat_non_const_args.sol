// ported-from: test/libsolidity/syntaxTests/constants/initialization/string_concat_non_const_args.sol
contract A {
    string name = "name";

    function getName() public view returns (string memory) {
        return name;
    }

    string public constant abName = string.concat("aaaa", "bbbb", name); //~ ERROR: initial value for constant variable has to be compile-time constant

    string public constant abgetName = string.concat("aaaa", "bbbb",getName()); //~ ERROR: initial value for constant variable has to be compile-time constant
}
