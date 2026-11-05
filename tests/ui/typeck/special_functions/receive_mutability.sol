// ported-from: test/libsolidity/syntaxTests/receiveEther/view_modifier.sol
// ported-from: test/libsolidity/syntaxTests/receiveEther/pure_modifier.sol

// Only `payable` receive functions are accepted, in every kind of contract.

contract ViewBody {
    uint x;
    receive() external view { x = 2; } //~ ERROR: `view` not allowed here; allowed values: payable
}

contract PureBody {
    uint x;
    receive() external pure { x = 2; } //~ ERROR: `pure` not allowed here; allowed values: payable
}

abstract contract AbstractView {
    receive() external view {} //~ ERROR: `view` not allowed here; allowed values: payable
}

abstract contract AbstractPure {
    receive() external pure {} //~ ERROR: `pure` not allowed here; allowed values: payable
}

interface InterfaceView {
    receive() external view; //~ ERROR: `view` not allowed here; allowed values: payable
}

interface InterfacePure {
    receive() external pure; //~ ERROR: `pure` not allowed here; allowed values: payable
}

contract NonPayable {
    receive() external {} //~ ERROR: receive ether function must be payable
}

abstract contract AbstractNonPayable {
    receive() external {} //~ ERROR: receive ether function must be payable
}

interface InterfaceNonPayable {
    receive() external; //~ ERROR: receive ether function must be payable
}

contract NonPayableParams {
    receive(uint256) external {}
    //~^ ERROR: receive ether function must be payable
    //~| ERROR: receive ether function cannot take parameters
}

abstract contract PayableParams {
    receive(uint256) external payable {} //~ ERROR: receive ether function cannot take parameters
}

interface InterfacePayableParams {
    receive(uint256) external payable; //~ ERROR: receive ether function cannot take parameters
}

contract Payable {
    receive() external payable {}
}

abstract contract AbstractPayable {
    receive() external payable {}
}

interface InterfacePayable {
    receive() external payable;
}
