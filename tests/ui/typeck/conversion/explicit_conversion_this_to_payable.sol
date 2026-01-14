contract C {
	function f() public pure {
		address payable p = payable(this);
		address payable q = payable(address(this));
	}
}
