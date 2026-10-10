pragma abicoder               v2;

contract C {
    struct Item {
        uint x;
    }

    function set(uint _x, string memory _y, Item memory _item, bool _z) external view {}
}
