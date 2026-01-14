contract S
{
	int o;
	function foo() public returns (int) { return o = 3; }
}

contract B is S
{
	function fii() public
	{
		o = S(super).foo();
	}
}
