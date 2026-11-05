// `erc7201` of a string literal or constant gives a base slot, like in solc. So does `erc7201` of a
// string that only the evaluation after type checking computes, which solc rejects with error 9796;
// see CONTRACT-001.
string constant NAME = "example.main";

contract Literal layout at erc7201("example.main") {}

contract Constant layout at erc7201(NAME) {}

contract Concat layout at erc7201(string.concat(NAME, ".v2")) {}
