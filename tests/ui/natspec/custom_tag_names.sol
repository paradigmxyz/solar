// ported-from: test/libsolidity/natspecJSON/invalid/invalid_tag.sol

/// @a&b test
//~^ WARN: invalid natspec tag '@a&b'
contract C {
	/// @custom:x^y test2
	//~^ WARN: invalid character in tag `@custom:x^y`
	function f() public pure {}
	/// @custom:
	//~^ WARN: tag `@custom:` does not contain a name
	function g() public pure {}
	/// @custom:abcDEF
	//~^ WARN: invalid character in tag `@custom:abcDEF`
	function h() public pure {}
	/// @custom:abc-def
	function i() public pure {}
	/// @custom
	//~^ WARN: invalid natspec tag '@custom'
	function j() public pure {}
}
