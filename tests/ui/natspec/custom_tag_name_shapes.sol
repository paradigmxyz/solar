// Custom tag names start with a lowercase letter and contain only lowercase letters and `-`.

contract C {
    /// @custom:my_tag Underscores are not allowed.
    //~^ WARN: invalid character in tag `@custom:my_tag`
    /// @custom:v2 Digits are not allowed.
    //~^ WARN: invalid character in tag `@custom:v2`
    /// @custom:-tag The name must start with a letter.
    //~^ WARN: invalid character in tag `@custom:-tag`
    /// @custom:tag- A trailing dash is fine.
    /// @custom:security-contact security@example.com
    function f() public pure {}
}

/// @custom:Unchecked Structs are not validated.
struct S {
    uint a;
}
