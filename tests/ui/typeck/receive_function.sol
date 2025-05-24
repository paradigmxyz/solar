library L {
    receive() external payable {}
    //
}

contract A {
    receive() external {}
    //
}

contract B {
    receive() external payable {}
}

contract C {
    receive() external payable {}
    receive() external payable {}
    //
}

contract D {
    receive() public payable {}
    //~^ ERROR: `public` not allowed here; allowed values: external
}

contract E {
    receive(uint256 x) external payable {}
    //
}

contract F {
    receive() external payable returns (uint256) {}
    //~^ ERROR: expected one of `;`, `external`, `internal`, `override`, `payable`, `private`, `public`, `pure`, `view`, `virtual`, or `{`, found keyword `returns`
}

contract G {
    receive() external view {}
    //
}

contract H {
    receive() external pure {}
    //
}

contract I {
    receive() external payable {
        // Valid receive function
    }
} 