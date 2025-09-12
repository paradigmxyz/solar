contract C {
    function f() external {
        assembly {
            number := 0
            //~^ ERROR: expected identifier, found Yul EVM builtin keyword `number`
            number, number := some_call()
            //~^ ERROR: expected identifier, found Yul EVM builtin keyword `number`
            //~| ERROR: expected identifier, found Yul EVM builtin keyword `number`
            let number := 0
            //~^ ERROR: expected identifier, found Yul EVM builtin keyword `number`
        }
    }
}
