//@compile-flags: -Ztypeck
// Test: delete on state variable modifies state

contract C {
    uint256 x;
    uint256[] arr;
    mapping(uint => uint) m;

    function deleteInView() public view {
        delete x;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function deleteArrayInView() public view {
        delete arr;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function deleteMappingInView() public view {
        delete m[0];
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function deleteInPure() public pure {
        uint local = 5;
        delete local;
    }
}
