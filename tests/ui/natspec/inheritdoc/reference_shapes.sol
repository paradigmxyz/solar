// `@inheritdoc` references resolve like solc's: as paths from the documented item's scope, with
// the rest of the line read as part of the reference.

import "./auxiliary/Interfaces.sol" as M;
import * as N from "./auxiliary/Interfaces.sol";

contract Namespaces is M.ERC20, N.ERC21 {
    /// @inheritdoc N.ERC21
    function transfer(address to, uint amount) external pure override(M.ERC20, N.ERC21) returns (bool) {
        return to == address(0) && amount == 0;
    }
}

contract Base {
    function f() public virtual {}
    function g() public virtual {}
    function h() public virtual {}
    function i() public virtual {}
}

contract Derived is Base, M.ERC20 {
    /// @inheritdoc M
    //~^ WARN: tag `@inheritdoc` reference `M` is not a contract
    function f() public override {}

    /// @inheritdoc M.Missing
    //~^ WARN: tag `@inheritdoc` references inexistent contract "M.Missing"
    function g() public override {}

    /// @inheritdoc Base.f
    //~^ WARN: tag `@inheritdoc` reference `Base.f` is not a contract
    function h() public override {}

    /// @inheritdoc Base and more text
    //~^ WARN: tag `@inheritdoc` references inexistent contract "Base and more text"
    function i() public override {}

    /// @inheritdoc M.ERC20
    function transfer(address to, uint amount) external pure returns (bool) {
        return to == address(0) && amount == 0;
    }
}
