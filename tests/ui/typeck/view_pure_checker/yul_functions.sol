contract C {
    function f() public view {
        assembly {
            function direct() {
                sstore(0, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }
            direct()

            function uncalled() {
                sstore(9, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }

            sstore(1, 1)
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state

            function outer() {
                function nested() {
                    tstore(0, 1)
                    //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                }
                nested()
            }
            outer()

            function leaf() {
                sstore(4, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }
            function interleaved() {
                tstore(4, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                leaf()
                sstore(5, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }
            interleaved()

            {
                function nested_block() {
                    sstore(2, 1)
                    //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                }
                nested_block()
                tstore(2, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }

            for {} 1 {
                function post() {
                    sstore(3, 1)
                    //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                }
                post()
            } {
                tstore(3, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }
        }
    }
}
