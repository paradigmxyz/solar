contract C {
    function f() public view {
        assembly {
            function direct() {
                sstore(0, 1)
            }
            direct()
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state

            function uncalled() {
                sstore(9, 1)
            }

            sstore(1, 1)
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state

            function outer() {
                function nested() {
                    tstore(0, 1)
                }
                nested()
            }
            outer()
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state

            function leaf() {
                sstore(4, 1)
            }
            function interleaved() {
                tstore(4, 1)
                leaf()
                sstore(5, 1)
            }
            interleaved()
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state

            {
                function nested_block() {
                    sstore(2, 1)
                }
                nested_block()
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                tstore(2, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }

            for {} 1 {
                function post() {
                    sstore(3, 1)
                }
                post()
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            } {
                tstore(3, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }
        }
    }
}
