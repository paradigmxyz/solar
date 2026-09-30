//@ compile-flags: -Zdump=secir

// Covers event emissions with indexed, dynamic, and anonymous arguments, per-argument
// dependencies, events reached through an internal call after an external call, and entry points
// that write storage without emitting an event.

contract Events {
    event Transfer(address indexed from, address indexed to, uint256 value);
    event Note(string text, uint256 indexed id, bytes32 tag);
    event Anonymous(uint256 value) anonymous;
    event Called(address target);

    mapping(address => uint256) balances;
    uint256 limit;

    function transfer(address to, uint256 value) external {
        balances[msg.sender] -= value;
        balances[to] += value;
        emit Transfer(msg.sender, to, value);
    }

    function note(string calldata text, bytes32 tag) external {
        emit Note(text, limit, tag);
    }

    function anon(uint256 v) external {
        emit Anonymous(v + limit);
    }

    function setLimit(uint256 l) external {
        limit = l;
    }

    function callThenLog(address target) external {
        (bool ok,) = target.call("");
        require(ok);
        _log(target);
    }

    function _log(address target) internal {
        emit Called(target);
    }
}
