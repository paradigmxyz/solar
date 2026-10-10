// ported-from: test/libsolidity/syntaxTests/structs/recursion/recursive_struct_function_pointer.sol

pragma abicoder               v2;
contract C {
    struct S {
        uint a;
        function() external returns (S memory) sub;
    }
    function f() public pure returns (S memory) {
    }
}
