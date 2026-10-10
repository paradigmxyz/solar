pragma abicoder               v2;

struct Data {
    bool flag;
}

contract A {
    function get() public view returns (Data memory) {}
}
