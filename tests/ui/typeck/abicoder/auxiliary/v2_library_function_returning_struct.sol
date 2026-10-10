pragma abicoder               v2;

library L {
    struct Item {
        uint x;
    }

    function get() external view returns(Item memory) {}
}
