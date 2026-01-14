contract F {
    receive() external payable returns (uint256) {}
    //~^ ERROR: expected one of `;`, `external`, `internal`, `override`, `payable`, `private`, `public`, `pure`, `view`, `virtual`, or `{`, found keyword `returns`
}
