contract C {
    function f() public view {
        assembly {
            function unused() {
                sstore(0, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }

            sstore(1, 1)
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state

            function outer() {
                function nested() {
                    tstore(0, 1)
                    //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                }
            }

            {
                function nested_block() {
                    sstore(2, 1)
                    //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                }
                tstore(2, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }

            for {} 1 {
                function post() {
                    sstore(3, 1)
                    //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
                }
            } {
                tstore(3, 1)
                //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            }
        }
    }
}
