contract C {
	function f() public view {
		address payable p = payable(msg.sender);
		address payable q = payable(address(msg.sender));
	}
}
