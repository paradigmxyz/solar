//@ignore-host: windows
//@compile-flags: --emit=mir

contract Storage {
    uint256 public count;

    function increment() public {
        count = count + 1;
    }

    function set(uint256 v) public {
        count = v;
    }

    function get() public view returns (uint256) {
        return count;
    }
}
