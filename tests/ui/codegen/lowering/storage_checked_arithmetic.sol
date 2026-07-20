//@compile-flags: -Zcodegen -Zdump=mir

contract StorageCheckedArithmetic {
    struct Account {
        uint128 small;
        int8 signed;
    }

    mapping(address => uint256) balance;
    mapping(address => Account) accounts;

    function storage_sub(address owner, uint256 amount) public {
        balance[owner] -= amount;
    }

    function storage_binary_sub(address owner, uint256 amount) public {
        balance[owner] = balance[owner] - amount;
    }

    function storage_struct_add(address owner, uint128 amount) public {
        accounts[owner].small += amount;
    }

    function storage_struct_signed_sub(address owner, int8 amount) public {
        accounts[owner].signed -= amount;
    }
}
