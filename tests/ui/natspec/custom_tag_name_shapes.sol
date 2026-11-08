// Custom tag names start with a lowercase letter and contain only lowercase letters and `-`.

contract C {
    /// @custom:my_tag Underscores are not allowed.
    /// @custom:v2 Digits are not allowed.
    /// @custom:-tag The name must start with a letter.
    /// @custom:tag- A trailing dash is fine.
    /// @custom:security-contact security@example.com
    //~^^^^^ WARN: invalid character in tag `@custom:my_tag`
    //~^^^^^ WARN: invalid character in tag `@custom:v2`
    //~^^^^^ WARN: invalid character in tag `@custom:-tag`
    function f() public pure {}
}

/// @custom:Unchecked Structs are not validated.
/// @custom
struct S {
    uint a;
}
