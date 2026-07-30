//@ revisions: homestead paris
//@[homestead] compile-flags: --evm-version homestead -Osize
//@[homestead] run-call: first() => 1
//@[homestead] run-call: second() => 2
//@[paris] compile-flags: --evm-version paris -Osize
//@[paris] run-call: first() => 1
//@[paris] run-call: second() => 2

contract McopySharedPreCancun {
    mapping(bytes => uint256) private values;

    constructor() {
        bytes memory a = new bytes(128);
        bytes memory b = new bytes(128);
        a[0] = 0x11;
        b[0] = 0x22;
        values[a] = 1;
        values[b] = 2;
    }

    function first() public view returns (uint256) {
        bytes memory key = new bytes(128);
        key[0] = 0x11;
        return values[key];
    }

    function second() public view returns (uint256) {
        bytes memory key = new bytes(128);
        key[0] = 0x22;
        return values[key];
    }
}
