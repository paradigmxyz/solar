//@ check-pass
// ported-from: test/libsolidity/syntaxTests/inheritance/override/correct_choice_for_base_function.sol

interface IBase {
    function foo() external view;
}

contract Base is IBase {
    function foo() public virtual view {}
}

interface IExt is IBase {}

contract Ext is IExt, Base {}

contract T {
    function foo() public virtual view {}
}

contract Impl is Ext, T {
    function foo() public view override(IBase, Base, T) {}
}
