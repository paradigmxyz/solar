//@ compile-flags: -Ztime-passes -Zcodegen -O none --emit=bin
//@ normalize-stderr-test: "time: +[0-9]+\.[0-9]{3}" -> "time: <TIME>"

contract TimePasses {
    function f() external pure {}
}
