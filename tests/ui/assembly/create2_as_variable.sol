// ported-from: test/libsolidity/syntaxTests/inlineAssembly/create2_as_variable_post_istanbul.sol

contract c {
    function f() public {
        uint create2; create2;
        assembly { pop(create2(0, 0, 0, 0)) }
    }
}
