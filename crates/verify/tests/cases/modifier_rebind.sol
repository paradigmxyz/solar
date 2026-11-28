pragma solidity ^0.8.20;
// An assignment inside a modifier argument can bind another name to the caller's object before
// the argument's value is taken, so the modifier may write that object.
contract ModifierRebind {
    function _set(uint256[1] memory part) private pure {
        part[0] = 1;
    }

    modifier touch(uint256[1] memory part) {
        _set(part);
        _;
    }

    /// @custom:ensures r == 2
    function _step(uint256[1][1] memory a, uint256[1] memory b)
        private
        pure
        touch((b = a[0])[0] == 0 ? b : b)
        returns (uint256 r)
    {
        r = 2;
    }

    // `touch` writes `a[0][0]` through `b`.
    /// @custom:ensures result == x
    function bad_rebind(uint256 x) external pure returns (uint256 result) {
        uint256[1][1] memory a;
        a[0][0] = x;
        uint256[1] memory s;
        _step(a, s);
        return a[0][0];
    }
}
