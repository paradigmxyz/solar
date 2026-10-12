contract C {
    function() external payable a;
    function() external public b;
    function() internal c;
    function() d;

    function() payable e; //~ ERROR: only external function types can be payable
    function() private payable h; //~ ERROR: invalid visibility

    function(function() public) internal i; //~ ERROR: invalid visibility
    function() returns (function() payable) j; //~ ERROR: only external function types can be payable
    mapping(uint => function() private) k; //~ ERROR: invalid visibility

    struct S {
        function() public f; //~ ERROR: invalid visibility
    }

    function l(function() public x) internal {} //~ ERROR: invalid visibility
}
