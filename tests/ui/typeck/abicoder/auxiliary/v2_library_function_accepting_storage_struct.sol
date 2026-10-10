pragma abicoder               v2;

library L {
    struct Item {
        uint x;
    }

    function get(Item storage _item) external view {}
}
