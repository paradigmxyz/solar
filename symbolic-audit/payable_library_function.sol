library P { function f() external payable {} function g() public payable returns (uint256) { return msg.value; } }
contract C { function h() external payable returns (uint256) { return msg.value; } }
