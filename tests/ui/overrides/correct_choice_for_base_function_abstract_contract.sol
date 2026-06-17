// ported-from: test/libsolidity/syntaxTests/inheritance/override/correct_choice_for_base_function_abstract_contract.sol

abstract contract IBase {
    function foo() external view virtual;
}

contract Base is IBase {
    function foo() public virtual override view {}
}

abstract contract IExt is IBase {}

contract Ext is IExt, Base {}

contract T {
    function foo() public virtual view {}
}

contract Impl is Ext, T {
    function foo() public view override(IBase, Base, T) {}
}
