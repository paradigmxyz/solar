// Overloaded functions and builtins have to be called to select an overload.
contract C {
    function g(uint) internal {}
    function g(bool) internal {}

    function f(bool c) public pure {
        function(bool) pure x;
        x = require; //~ ERROR: no matching declarations found
        (require); //~ ERROR: no matching declarations found
        require == require; //~ ERROR: no matching declarations found
        //~^ ERROR: no matching declarations found
        c ? require : require; //~ ERROR: no matching declarations found
        //~^ ERROR: no matching declarations found
        require.selector; //~ ERROR: no matching declarations found
        g; //~ ERROR: no matching declarations found

        // Not overloaded.
        assert;
        require(c);
        require(c, "message");
        revert();
    }
}
