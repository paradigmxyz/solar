contract C {
    enum ActionChoices { GoLeft, GoRight }
    function f() public pure {
        ActionChoices.GoLeft(); //~ ERROR: expected function
    }
}
