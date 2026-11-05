// ported-from: test/libsolidity/syntaxTests/inlineAssembly/invalid/invalid_number.sol

// Like solc, Yul numbers are plain decimal or `0x` hexadecimal integers. Scanner errors have the
// code 8936 instead of 1465; see PARSE-007.
contract C {
    function f() public pure {
        assembly {
            let a := 1__0 //~ ERROR: invalid number literal
            let b := 1_0 //~ ERROR: invalid number literal
            let c := 1_ //~ ERROR: invalid number literal
            let d := 0x1_0 //~ ERROR: invalid number literal
            let e := 0x1_ //~ ERROR: invalid number literal
            let f := 0x1__0 //~ ERROR: invalid number literal
            let g := 1e5 //~ ERROR: invalid number literal
            let h := 1_e5 //~ ERROR: invalid number literal
            let i := 1.5 //~ ERROR: invalid number literal
            let j := 0x_1 //~ ERROR: hexadecimal digit missing or invalid
            let k := 0x //~ ERROR: hexadecimal digit missing or invalid
            let l := 0b //~ ERROR: identifier-start is not allowed at end of a number
            let m := 0b1 //~ ERROR: identifier-start is not allowed at end of a number
            let n := 0b2 //~ ERROR: identifier-start is not allowed at end of a number
            let o := 0o //~ ERROR: identifier-start is not allowed at end of a number
            let p := 0o7 //~ ERROR: identifier-start is not allowed at end of a number
            let q := 0_1 //~ ERROR: identifier-start is not allowed at end of a number
            let r := 0100 //~ ERROR: octal numbers not allowed
            switch 1__0 //~ ERROR: invalid number literal
            case 1_0 {} //~ ERROR: invalid number literal
            default {}
            if 1_0 {} //~ ERROR: invalid number literal
            let s := 1000
            let t := 0x10
            let u := 0xAbC
            let v := 0
            let w := 0x0
        }
    }
}
