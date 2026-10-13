contract C {
    function f(address a) public pure {
        payable({value: a}); //~ ERROR: type conversions cannot take named arguments
        address({value: a}); //~ ERROR: type conversions cannot take named arguments
        C({value: a}); //~ ERROR: type conversions cannot take named arguments
    }
}
