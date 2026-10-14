function f() pure {
    assembly ("memory-safé") {} //~ ERROR: unicode characters are not allowed in string literals
    //~^ WARN: unknown inline assembly flag
    assembly ("memory-safe", "memory-safe\q") {} //~ ERROR: unknown character escape
    //~^ WARN: unknown inline assembly flag
    assembly "evmasmé" {} //~ ERROR: unicode characters are not allowed in string literals
}
