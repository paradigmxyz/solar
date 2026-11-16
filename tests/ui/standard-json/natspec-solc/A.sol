// The documentation outputs follow solc in which comments, tags, events and errors they list.

/// @notice Userdoc for file-level event E.
/// @dev Devdoc for file-level E.
event E();

library L {
    /// @notice Defined in library L
    /// @dev Library event
    event LibraryEvent(uint32);

    /// @notice Library error
    error Failed(uint256 a);
}

contract A {
    /// Something failed.
    /// @param x first parameter
    error F(uint256 x);
}

contract C {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    /// @return a first
    /// @return b second
    Pair public pair;

    /// @return the value
    uint256 public value;

    /// @notice X failed.
    /// @param y first parameter
    error F(uint256 y);

    ///
    /// @notice ignored

    /// @notice last comment
    function f() public {
        emit E();
        emit L.LibraryEvent(1);
    }

    /// @dev
    /// Multiplies by 7
    function g(bool b) public pure {
        if (b) revert F(1);
        revert A.F(2);
    }

    function h() public pure {
        revert L.Failed(3);
    }
}

contract B {
    /// @notice base k
    function k() public virtual {}

    /// @notice base m
    function m() public virtual {}
}

contract D is B {
    /// @notice ignored, so k inherits

    ///
    function k() public override {}

    /// @inheritdoc B

    /// @dev only this
    function m() public override {}
}
