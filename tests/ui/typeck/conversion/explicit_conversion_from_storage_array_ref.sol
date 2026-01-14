contract C {
	int[10] x;
	function f() public view {
		int[](x);
		int(x);
	}
}
