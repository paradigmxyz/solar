pragma abicoder               v2;

contract C {
    struct Item {
        uint x;
    }

    function get(Item memory) external view {}
}
