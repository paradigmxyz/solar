// ported-from: test/libsolidity/ASTJSON/enum_value_natspec.sol
enum Color {
    Red,
    /// @notice example of notice
    /// @dev example of dev
    Green,
    /// @dev example of dev
    Blue
    /// @dev beyond last value
}


// A doc comment that a blank line separates from the value, a block doc comment and a doc comment
// after a comma.
enum Shade {
    /// @dev ends before the blank line

    Light,
    /** @notice block doc */
    Mid, /// @dev after the comma
    Dark
}
