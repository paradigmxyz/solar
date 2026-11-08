contract C {
    function f() pure public { selfdestruct; } //~ WARN: statement has no effect
}
