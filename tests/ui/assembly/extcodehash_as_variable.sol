// ported-from: test/libsolidity/syntaxTests/inlineAssembly/extcodehash_as_variable_post_constantinople.sol

contract c {
    function f() public view {
        uint extcodehash;
        extcodehash;
        assembly { pop(extcodehash(0)) }
    }
}
