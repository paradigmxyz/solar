import "./auxiliary/Interfaces.sol" as M;

contract Base {
    function f() public virtual {}
    function g() public virtual {}
    function h() public virtual {}
    function i() public virtual {}
    function j() public virtual {}
    function o(uint) public virtual {}
    function o(bool) public virtual {}
    function k() public virtual {}
}

contract Derived is Base, M.ERC20 {
    /// @inheritdoc M
    //~^ WARN: tag `@inheritdoc` references inexistent contract "M"
    function f() public override {}

    /// @inheritdoc M.Missing
    //~^ WARN: tag `@inheritdoc` references inexistent contract "M.Missing"
    function g() public override {}

    /// @inheritdoc Base.f
    //~^ WARN: tag `@inheritdoc` references inexistent contract "Base.f"
    function h() public override {}

    /// @inheritdoc Base.
    //~^ WARN: tag `@inheritdoc` references inexistent contract "Base."
    function i() public override {}

    /// @inheritdoc Base.o
    //~^ WARN: tag `@inheritdoc` references inexistent contract "Base.o"
    function j() public override {}

    /// @inheritdoc
    //~^ WARN: tag `@inheritdoc` references inexistent contract ""
    function k() public override {}

    /// @inheritdoc M.ERC20
    function transfer(address to, uint amount) external pure returns (bool) {
        return to == address(0) && amount == 0;
    }
}

contract TrailingText is Base {
    // The reference is the first word, so `Base` resolves.
    /// @inheritdoc Base and more text
    function f() public override {}
}
