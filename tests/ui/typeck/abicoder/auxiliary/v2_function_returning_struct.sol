pragma abicoder               v2;

contract C {
    struct Item {
        uint x;
    }

    function get() external view returns(Item memory) {}
}
