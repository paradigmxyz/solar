//@ compile-flags: -Ztime-passes -O none --emit=bin --evm-version byzantium -Zevm-ir-pipeline=legalize-shifts
//@ normalize-stderr-test: "time: +[0-9]+\.[0-9]{3}" -> "time: <TIME>"

contract TimePasses {
    function f() external pure {}
}
