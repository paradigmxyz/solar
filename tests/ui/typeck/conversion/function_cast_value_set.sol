//@compile-flags: -Ztypeck
contract C {
    function f() public payable {
        function() external payable x = this.f{value: 7}; //~ ERROR: call options must be part of a call expression
    }
}
