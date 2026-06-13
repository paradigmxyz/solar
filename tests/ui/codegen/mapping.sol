//@ignore-host: windows
//@compile-flags: --emit=mir

contract Mapping {
    mapping(uint256 => uint256) public balances;
    mapping(address => mapping(address => uint256)) public allowances;

    function set_balance(uint256 id, uint256 amount) public {
        balances[id] = amount;
    }

    function add_balance(uint256 id, uint256 amount) public {
        balances[id] = balances[id] + amount;
    }

    function approve(address owner, address spender, uint256 amount) public {
        allowances[owner][spender] = amount;
    }

    function get_allowance(address owner, address spender) public view returns (uint256) {
        return allowances[owner][spender];
    }
}
