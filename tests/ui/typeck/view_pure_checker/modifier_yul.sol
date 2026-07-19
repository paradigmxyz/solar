contract C {
    modifier readsInYul() {
        assembly {
            function read() {
                pop(sload(0))
            }
            read()
        }
        _;
    }

    modifier writesInYul() {
        assembly {
            function write() {
                sstore(0, 1)
            }
            write()
        }
        _;
    }

    function pureRead1() public pure readsInYul {
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }

    function pureRead2() public pure readsInYul {
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }

    function viewWrite() public view writesInYul {
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }
}
