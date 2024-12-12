library L1 {
    receive() external payable {} //~ERROR: libraries cannot have receive ether functions
}

contract C3 {
    receive() external {} //~ERROR: receive ether function must be payable
}

contract C4 {
    receive() external payable {}
}

contract C9 {
    receive(bool _x) external payable {} //~ERROR: receive ether function cannot take parameters
}
