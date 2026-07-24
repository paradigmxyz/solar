//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract Mapping {
    // CHECK-LABEL: fn @balances{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: sload [[SLOT]]
    mapping(uint256 => uint256) public balances;

    // CHECK-LABEL: fn @allowances{{[( ]}}
    // CHECK: [[OWNER:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[SPENDER:v[0-9]+]] = mapping_slot arg1, [[OWNER]]
    // CHECK: sload [[SPENDER]]
    mapping(address => mapping(address => uint256)) public allowances;

    // CHECK-LABEL: fn @set_balance{{[( ]}}
    // CHECK: [[SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: sstore [[SLOT]], arg1
    function set_balance(uint256 id, uint256 amount) public {
        balances[id] = amount;
    }

    // CHECK-LABEL: fn @add_balance{{[( ]}}
    // CHECK: [[READ_SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: [[OLD:v[0-9]+]] = sload [[READ_SLOT]]
    // CHECK: [[NEW:v[0-9]+]] = add [[OLD]], arg1
    // CHECK: [[WRITE_SLOT:v[0-9]+]] = mapping_slot arg0, 0
    // CHECK: sstore [[WRITE_SLOT]], [[NEW]]
    function add_balance(uint256 id, uint256 amount) public {
        balances[id] = balances[id] + amount;
    }

    // CHECK-LABEL: fn @approve{{[( ]}}
    // CHECK: [[OWNER:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[SPENDER:v[0-9]+]] = mapping_slot arg1, [[OWNER]]
    // CHECK: sstore [[SPENDER]], arg2
    function approve(address owner, address spender, uint256 amount) public {
        allowances[owner][spender] = amount;
    }

    // CHECK-LABEL: fn @get_allowance{{[( ]}}
    // CHECK: [[OWNER:v[0-9]+]] = mapping_slot arg0, 1
    // CHECK: [[SPENDER:v[0-9]+]] = mapping_slot arg1, [[OWNER]]
    // CHECK: sload [[SPENDER]]
    function get_allowance(address owner, address spender) public view returns (uint256) {
        return allowances[owner][spender];
    }
}
