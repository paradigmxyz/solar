//@ check-pass
//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

contract CompoundAssign {
    uint256 public value;

    function add_to_value(uint256 x) public {
        value += x;
    }

    function sub_from_value(uint256 x) public {
        value -= x;
    }

    function mul_value(uint256 x) public {
        value *= x;
    }

    function bump_post() public returns (uint256) {
        return value++;
    }

    function bump_pre() public returns (uint256) {
        return ++value;
    }
}
