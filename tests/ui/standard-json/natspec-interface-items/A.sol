interface IEvents {
    /// @notice Emitted on transfer
    /// @dev Transfer details
    /// @param amount The amount
    event Transfer(uint amount);

    /// @notice Never emitted
    event Unused(uint amount);

    /// @notice Insufficient funds
    /// @dev Error details
    error Insufficient(uint needed);
}

library Lib {
    /// @notice Emitted by the library
    event Logged(uint value);

    /// @notice Same signature as a used event of another interface
    event Clash(uint value);

    /// @notice Library failure
    error Failed(uint code);

    function log(uint v) internal {
        emit Logged(v);
    }
}

interface IOther {
    /// @notice Clash in another interface
    event Clash(uint value);
}

// Emits and reverts with items declared outside of its hierarchy.
contract Used {
    function f() external {
        emit IEvents.Transfer(1);
        Lib.log(2);
        revert IEvents.Insufficient(3);
    }

    function g() external pure {
        revert Lib.Failed(4);
    }
}

// Used events with one signature are dropped.
contract Clashing {
    function f() external {
        emit Lib.Clash(1);
        emit IOther.Clash(2);
    }
}

// A defined event wins over a used event with the same signature.
contract Defined {
    /// @notice Defined clash
    event Clash(uint value);

    function f() external {
        emit Defined.Clash(1);
        emit IOther.Clash(2);
    }
}

// Items of its bases are listed whether or not it uses them.
contract Unreferenced is IEvents {}
