//@ compile-flags: -Zcodegen --emit=mir

contract PackedBool {
    bool public a;
    bool public b;

    function set(bool x, bool y) external {
        a = x;
        b = y;
    }

    function both() external view returns (bool) {
        return a && b;
    }
}
