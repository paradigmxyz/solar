//@ revisions: creation runtime maps resources
//@ compile-flags: --pretty-json --allow 2264
//@[creation] compile-flags: --emit=ethdebug
//@[runtime] compile-flags: --emit=ethdebug-runtime
//@[maps] compile-flags: --emit=srcmap,srcmap-runtime
//@[resources] compile-flags: --emit=ethdebug-resources
//@ normalize-stdout-test: "solar-[0-9a-f]{64}" -> "solar-COMPILATION_ID"

contract C {
    function f() external {}
}
