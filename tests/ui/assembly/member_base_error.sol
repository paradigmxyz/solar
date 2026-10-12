// Suffixes on bases that already failed to type check do not report a second error.

contract C {
    function f() public view {
        Missing storage ptr; //~ ERROR: unresolved symbol `Missing`
        assembly {
            pop(ptr.slot)
            pop(missing.slot) //~ ERROR: unresolved symbol `missing`
        }
    }
}
