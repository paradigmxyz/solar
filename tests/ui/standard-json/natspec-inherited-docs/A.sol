interface I {
    /// @notice Does f.
    function f() external;

    /// @notice Does g.
    /// @param a The a.
    function g(uint a) external;
}

interface J {
    /// @notice Does f in J.
    function f() external;
}

// An override without documentation inherits its base's.
contract C is I {
    function f() external override {}
    function g(uint a) external override {}
}

// An override of several bases inherits nothing, nor does one with other parameter names.
contract D is I, J {
    function f() external override(I, J) {}
    function g(uint b) external override {}
}
