contract C {
    struct S {
        function() private s; //~ ERROR: invalid visibility
    }

    function(function() public) internal f; //~ ERROR: invalid visibility
    function() returns (function() private) g; //~ ERROR: invalid visibility
    mapping(uint => function() private) h; //~ ERROR: invalid visibility

    function i(function(uint[] storage) public x) internal {} //~ ERROR: invalid visibility
    // solc stops after its fatal 6012 error; we lower the types as internal and keep checking.
    function j(function() private y) public {} //~ ERROR: invalid visibility
    //~^ ERROR: internal function pointers
    function k() public view returns (function() public) {} //~ ERROR: invalid visibility
    //~^ ERROR: internal function pointers
}
