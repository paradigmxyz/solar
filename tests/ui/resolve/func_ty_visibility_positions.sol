contract C {
    struct S {
        function() private s; //~ ERROR: invalid visibility
    }

    function(function() public) internal f; //~ ERROR: invalid visibility
    function() returns (function() private) g; //~ ERROR: invalid visibility
    mapping(uint => function() private) h; //~ ERROR: invalid visibility

    function i(function(uint[] storage) public x) internal {} //~ ERROR: invalid visibility
    function j(function() private y) public {} //~ ERROR: invalid visibility
    function k() public view returns (function() public) {} //~ ERROR: invalid visibility
}
