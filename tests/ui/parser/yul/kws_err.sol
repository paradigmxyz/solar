contract C {
    function f() external {
        assembly {
            number := 0
            //~^ ERROR: builtin function `number` must be called
            number, number := some_call()
            //~^ ERROR: builtin function `number` must be called
            //~| ERROR: builtin function `number` must be called
            //~| ERROR: unresolved symbol `some_call`
            let number := 0
            //~^ ERROR: `number` is reserved for a Yul builtin
        }
    }
}
