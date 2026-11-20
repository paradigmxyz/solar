//@ compile-flags: -Onone --emit=srcmap-runtime,ethdebug-runtime
//@ normalize-stdout-test: "(?s).+" -> ""

// An unoptimized function whose only code is a call to a function tagged for another objective
// that never returns falls through into it, and both entries share the first instruction. The
// assembler marks one and does not panic.
contract C {
    function outer() external pure {
        inner();
    }

    /// @custom:solar-optimize gas
    function inner() internal pure {
        assembly {
            stop()
        }
    }
}
