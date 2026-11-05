contract C {
    uint256 public a = 0x123 ether; //~ ERROR: cannot be used with unit denominations
    uint256 public b = 0x123 days; //~ ERROR: cannot be used with unit denominations
    uint256 public h = 1 years; //~ ERROR: unit denomination is deprecated
    uint256 public n = 1e+ seconds;
    //~^ ERROR: expected at least one digit in exponent
    //~| ERROR: `+` is not allowed in the exponent

    // OK
    uint256 public i = 1 seconds;
    uint256 public j = 1 minutes;
    uint256 public k = 1 hours;
    uint256 public l = 1 days;
    uint256 public m = 1 weeks;
}
