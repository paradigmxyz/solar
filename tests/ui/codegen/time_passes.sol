//@ codegen-matrix: standard
//@ compile-flags: -Ztime-passes
//@ normalize-stderr-test: "time: +[0-9]+\.[0-9]{3}" -> "time: <TIME>"

contract TimePasses {
    function f() external pure {}
}
