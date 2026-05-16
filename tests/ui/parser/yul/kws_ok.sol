contract C {
    uint number;
    function f() external {
        assembly {
            function some_call() -> a, b {
                a := number()
                b := number()
            }

            number.slot := 69
            number.slot, number.slot := some_call()

            number.number := 69
            number.number, number.number := some_call()

            sstore(number.slot, 1)

            pop(number())
        }
    }
}
