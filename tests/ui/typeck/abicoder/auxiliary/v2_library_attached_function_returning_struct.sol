pragma abicoder               v2;

library L {
    struct Item {
        uint x;
    }

    function f(uint) external view returns (Item memory) {}
}
