pragma abicoder               v2;

contract C {
    struct Item {
        uint[] y;
    }

    function get() external view returns(Item memory) {}
}
