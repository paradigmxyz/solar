// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

enum Level { Low, Mid, High }
enum Color { Red, Green, Blue }

contract Base {
    enum Status { Off, On, Paused }
}

/// Enum members in specs resolve where the spec is written: this contract's enums, its bases'
/// and the file's, with a contract's own enum shadowing a file-level one of the same name.
contract Derived is Base {
    enum Mode { A, B, C, D }
    // Shadows the file-level `Color` here: `Color.Red` is 1, not 0.
    enum Color { Blue, Red }

    Status status;
    Mode mode;
    Level level;
    uint8 n;

    /// @custom:ensures status == Status.Paused && mode == Mode.D && level == Level.High
    function set() external { status = Status.Paused; mode = Mode.D; level = Level.High; }

    /// @custom:ensures n == Color.Red
    function red() external { n = 1; }
}
