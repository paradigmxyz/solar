contract C {
    function() external payable a;
    function() payable b; //~ ERROR: only external function types can be payable
    function() returns (function() payable) c; //~ ERROR: only external function types can be payable
    function() private payable d; //~ ERROR: invalid visibility
}
