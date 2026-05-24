// ported-from: test/libsolidity/syntaxTests/metaTypes/super_name.sol

contract A {
    function f() public pure {
        type(super).name; //~ ERROR: expected item, found builtin
    }
}
