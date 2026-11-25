pragma solidity ^0.8.20;
contract A1 {
    uint256 public x;
    modifier inc() { x += 1; _; }
    constructor() inc { x += 10; }
}
contract BadMod is A1 {
    /// base constructor's modifier runs: x == 11
    /// @custom:ensures x == 10
    constructor() {}
}
contract OkMod is A1 {
    /// @custom:ensures x == 11
    constructor() {}
}
contract A2 {
    uint256 public x;
    modifier gate() { require(x == 1); _; }
    constructor() gate { x = 5; }
}
contract BadGate is A2 {
    /// A2's constructor always reverts (x is 0), so this never returns; succeeds_if true is false
    /// @custom:succeeds_if true
    constructor() {}
}
