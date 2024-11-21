// https://github.com/foundry-rs/foundry/issues/9349

pragma solidity >=0.4.22 <0.6;

contract BugReport {
    function() external payable { //~ ERROR: expected a state variable declaration
        deposit();
        uint
    } //~ ERROR: expected
    function deposit() public payable {}
}
