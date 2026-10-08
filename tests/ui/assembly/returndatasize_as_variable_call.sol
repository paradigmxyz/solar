// ported-from: test/libsolidity/syntaxTests/inlineAssembly/returndatasize_as_variable_call_post_byzantium.sol.sol

contract C {
    function f() public pure {
        uint returndatasize;
        returndatasize;
        assembly {
            let x := returndatasize()
        }
    }
}
