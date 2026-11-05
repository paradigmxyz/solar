interface I {
    /// @notice Does f
    /// @dev Details of f
    /// @param x The x
    /// @return r The r
    /// @custom:note Not inherited
    function f(uint x) external returns (uint r);

    /// @notice Does g
    function g() external;

    /// @notice Does h
    /// @param a The a
    function h(uint a) external;

    /// @notice Does k
    function k() external;
}

interface J {
    /// @notice Does g in J
    function g() external;
}

// An override without docs inherits all inheritable tags of its base.
contract Plain is I {
    function f(uint x) external virtual override returns (uint r) {}
    function g() external virtual override {}
    function h(uint a) external virtual override {}
    function k() external virtual override {}
}

// An override with different parameter names inherits nothing, and one with
// different return names renames the inherited `@return` tag.
contract Renamed is I {
    function f(uint x) external override returns (uint y) {}
    function g() external override {}
    function h(uint b) external override {}
    function k() external override {}
}

// An override with any tag of its own inherits nothing.
contract Partial is I {
    /// @notice Own notice
    function f(uint x) external override returns (uint r) {}

    /// @custom:note Own custom tag
    function g() external override {}

    function h(uint a) external override {}
    function k() external override {}
}

// `@inheritdoc` fills in only the missing tags.
contract Inheritdoc is I {
    /// @notice Own notice
    /// @inheritdoc I
    function f(uint x) external override returns (uint r) {}

    function g() external override {}
    function h(uint a) external override {}
    function k() external override {}
}

// An override of several bases inherits nothing without `@inheritdoc`.
contract Multiple is I, J {
    function f(uint x) external override returns (uint r) {}
    function g() external override(I, J) {}
    function h(uint a) external override {}
    function k() external override {}
}

abstract contract Left is I {
    function f(uint x) external virtual override returns (uint r) {}
    function g() external virtual override {}
}

abstract contract Right is I {
    function f(uint x) external virtual override returns (uint r) {}
    function g() external virtual override {}
}

// Overrides of `Left` and `Right` have two distinct bases, except where a base is inherited from
// the same declaration through both.
abstract contract Diamond is Left, Right {
    function f(uint x) external override(Left, Right) returns (uint r) {}
    function g() external override(Left, Right) {}
}

abstract contract Via1 is I {}

abstract contract Via2 is I {}

// Both paths reach the same base declaration, so there is one base.
contract Same is Via1, Via2 {
    function f(uint x) external override returns (uint r) {}
    function g() external override {}
    function h(uint a) external override {}
    function k() external override {}
}

// Inheritance chains through intermediate overrides.
contract Chain is Plain {
    function f(uint x) external override returns (uint r) {}
    function g() external override {}
    function h(uint a) external override {}
    function k() external override {}
}

interface V {
    /// @notice Gets the count
    /// @dev Details of count
    /// @return v The count
    function count() external view returns (uint v);

    /// @notice Gets the total
    /// @return The total
    function total() external view returns (uint);

    /// @notice Gets the owner
    function owner(uint id) external view returns (address);
}

// A public state variable inherits the documentation of the function it overrides.
contract Variables is V {
    uint public override count;
    uint public override total;
    mapping(uint => address) public override owner;
}

contract DocumentedVariable is V {
    /// @notice Own notice
    uint public override count;
    /// @inheritdoc V
    uint public override total;
    mapping(uint => address) public override owner;
}
