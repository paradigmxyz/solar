//@ compile-flags: -Zprint-natspec

contract Base {
    /// @notice Base function notice
    //~^ NOTE: @notice Base function notice
    /// @dev Base function dev
    //~^ NOTE: @dev Base function dev
    /// @param x The x parameter from base
    //~^ NOTE: @param x The x parameter from base
    /// @param y The y parameter from base
    //~^ NOTE: @param y The y parameter from base
    /// @return success Whether the operation succeeded
    //~^ NOTE: @return success Whether the operation succeeded
    /// @return value The result value
    //~^ NOTE: @return value The result value
    /// @custom:security Audited by Base team
    //~^ NOTE: @custom:security Audited by Base team
    function foo(uint x, uint y) public virtual returns (bool success, uint value) { //~ ERROR: resolved NatSpec for function `Base.foo`
        return (true, x + y);
    }
}

contract Child1 is Base {
    /// @inheritdoc Base
    //~^ NOTE: inherits NatSpec from function `Base.foo(uint256,uint256)`
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) { //~ ERROR: resolved NatSpec for function `Child1.foo`
        return (true, x * y);
    }
}

contract Child2 is Base {
    /// @notice Child2 notice
    //~^ NOTE: @notice Child2 notice
    /// @dev Child2 dev
    //~^ NOTE: @dev Child2 dev
    /// @inheritdoc Base
    //~^ NOTE: inherits NatSpec from function `Base.foo(uint256,uint256)`
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) { //~ ERROR: resolved NatSpec for function `Child2.foo`
        return (false, 0);
    }
}

contract Child3 is Base {
    /// @param x The x parameter from child3
    //~^ NOTE: @param x The x parameter from child3
    /// @inheritdoc Base
    //~^ NOTE: inherits NatSpec from function `Base.foo(uint256,uint256)`
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) { //~ ERROR: resolved NatSpec for function `Child3.foo`
        return (true, x);
    }
}

contract Child4 is Base {
    /// @return success Child4 override for success
    //~^ NOTE: @return success Child4 override for success
    /// @inheritdoc Base
    //~^ NOTE: inherits NatSpec from function `Base.foo(uint256,uint256)`
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) { //~ ERROR: resolved NatSpec for function `Child4.foo`
        return (false, x + y);
    }
}

contract Child5 is Base {
    /// @custom:audit Reviewed by Child5 auditor
    //~^ NOTE: @custom:audit Reviewed by Child5 auditor
    /// @inheritdoc Base
    //~^ NOTE: inherits NatSpec from function `Base.foo(uint256,uint256)`
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) { //~ ERROR: resolved NatSpec for function `Child5.foo`
        return (true, y);
    }
}

contract GrandChild is Child1 {
    /// @inheritdoc Child1
    //~^ NOTE: inherits NatSpec from function `Child1.foo(uint256,uint256)`
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) { //~ ERROR: resolved NatSpec for function `GrandChild.foo`
        return (true, x - y);
    }
}
