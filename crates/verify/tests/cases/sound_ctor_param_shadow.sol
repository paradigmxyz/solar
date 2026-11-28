pragma solidity ^0.8.20;
contract S { uint256 internal x = 3; }
contract BadShadowInit is S {
    uint256 public y;
    /// y is the argument, not the state x
    /// @custom:ensures y == 3
    constructor(uint256 x) { y = x; }
}
contract OkShadowInit is S {
    uint256 public y;
    /// @custom:ensures y == old(x)
    constructor(uint256 x) { y = x; }
}
contract S2 { uint256 internal x; uint256 public y; constructor() { y = x + 1; } }
contract BadShadowBase is S2 {
    /// S2 reads the state x (0), so y == 1
    /// @custom:ensures y == x + 1
    constructor(uint256 x) { x; }
}
contract S3 { uint256 internal x; constructor() { x = 9; } }
contract BadShadowBaseW is S3 {
    uint256 public z;
    /// base writes state x; z copies the parameter
    /// @custom:ensures z == 9
    constructor(uint256 x) { z = x; }
}
contract OkNoShadowInit is S {
    uint256 public y;
    /// @custom:ensures y == p
    constructor(uint256 p) { y = p; }
}
contract OkNoShadowBaseW is S3 {
    uint256 public z;
    /// @custom:ensures z == p && x == 9
    constructor(uint256 p) { z = p; }
}
