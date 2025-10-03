library L {
    receive() external payable {}
    //~^ ERROR: libraries cannot have receive ether functions
}

contract A {
    receive() external {}
    //~^ ERROR: receive ether function must be payable
}

contract B {
    receive() external payable {}
}

contract D {
    receive() public payable {}
    //~^ ERROR: `public` not allowed here; allowed values: external
}

contract E {
    receive(uint256 x) external payable {}
    //~^ ERROR: receive ether function cannot take parameters
}

contract G {
    receive() external view {}
    //~^ ERROR: `view` not allowed here; allowed values: payable
}

contract H {
    receive() external pure {}
    //~^ ERROR: `pure` not allowed here; allowed values: payable
}

contract I {
    receive() external payable {
        // Valid receive function
    }
} 
