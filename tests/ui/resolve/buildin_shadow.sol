//https://github.com/argotorg/solidity/blob/develop/test/libsolidity/syntaxTests/events/illegal_names_exception.sol

contract C {
	event this();
	event super();
	event _();
}