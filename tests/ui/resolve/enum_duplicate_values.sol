// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/133_enum_duplicate_values.sol

    contract test {
        enum ActionChoices { GoLeft, GoRight, GoLeft, Sit } //~ ERROR: identifier `GoLeft` already declared
    }
