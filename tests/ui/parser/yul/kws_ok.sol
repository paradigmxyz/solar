contract C {
    uint number;
    function f() external {
        assembly {
            function some_call() -> a, b {
                a := number()
                b := number()
            }

            number.slot := 69 //~ ERROR: state variables cannot be assigned to in inline assembly
            number.slot, number.slot := some_call() //~ ERROR: state variables cannot be assigned to in inline assembly
            //~^ ERROR: state variables cannot be assigned to in inline assembly

            number.number := 69 //~ ERROR: storage variables only support `.slot` and `.offset`
            number.number, number.number := some_call() //~ ERROR: storage variables only support `.slot` and `.offset`
            //~^ ERROR: storage variables only support `.slot` and `.offset`

            sstore(number.slot, 1)

            pop(number())
        }
    }
}
