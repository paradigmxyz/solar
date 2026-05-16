contract C {
    uint number;
    function f() external {
        assembly {
            function some_call() -> a, b {
                a := number()
                b := number()
            }

            let a := number()
            a, a := some_call()

            sstore(0, number())

            pop(number())
        }
    }
}
