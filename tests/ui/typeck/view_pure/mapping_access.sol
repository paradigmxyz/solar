//@compile-flags: -Ztypeck
// Test: mapping access requires view, modification requires non-view

contract C {
    mapping(uint => uint) a;

    function readMapping() public view {
        a;
    }

    function indexMapping() public view {
        a[2];
    }

    function writeMapping() public {
        a[2] = 3;
    }

    function pureReadsMapping() public pure returns (uint) {
        return a[0];
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires "view"
    }

    function viewWritesMapping() public view {
        a[1] = 5;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }
}
