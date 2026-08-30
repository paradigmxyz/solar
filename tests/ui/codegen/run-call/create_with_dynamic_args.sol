//@ run-call: Factory::makeAndRead() => 5, 18

// `new` with dynamic constructor arguments appends their ABI encoding after
// the creation code in the CREATE payload.

contract Token {
    string public name;
    uint8 public decimals;

    constructor(string memory n, uint8 d) {
        name = n;
        decimals = d;
    }
}

contract Factory {
    function makeAndRead() external returns (uint256, uint8) {
        Token t = new Token("hello", 18);
        return (bytes(t.name()).length, t.decimals());
    }
}
