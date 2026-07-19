contract C {
    function uncalledRead() public {
        //~^ WARN: function state mutability can be restricted to pure
        assembly {
            function read() {
                pop(sload(0))
            }
        }
    }

    function builtinOrder() public pure {
        assembly {
            sstore(sload(0), caller())
            //~^ ERROR: function cannot be declared as pure because this expression (potentially) modifies the state
            //~| ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
            //~| ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
        }
    }

    function recursive() public view {
        assembly {
            function self() {
                sstore(1, 1)
                self()
            }
            self()
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
            self()
        }
    }

    function mutuallyRecursive() public pure {
        assembly {
            function a() {
                pop(sload(2))
                b()
            }
            function b() {
                pop(caller())
                a()
            }
            a()
            //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
        }
    }
}
