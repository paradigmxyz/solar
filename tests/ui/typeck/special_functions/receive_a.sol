contract C {
    receive() external payable {}
    receive() external payable {}
    //~^ ERROR: receive function already declared
}
