contract LT {
    uint256 a = 1000_; //~ERROR: invalid use of underscores in number literal
    uint256 b = 100__0;//~ERROR: invalid use of underscores in number literal
    uint256 c = 1_.4e10;//~ERROR: invalid use of underscores in number literal
    uint256 d = 3.4_e10;//~ERROR: invalid use of underscores in number literal
    uint256 e = 3.4e_10;//~ERROR: invalid use of underscores in number literal

    // exception
    uint256 f = 3._4e10; // Does not show up
}
