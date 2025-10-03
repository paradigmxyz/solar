function f() {
    assembly {
        switch 42 //~ ERROR: `switch` statement has no cases

        switch 42 //~ WARN: `switch` statement has only a default case
        default {}
    }
}
