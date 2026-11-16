//@revisions: homestead byzantium
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
//@[byzantium] compile-flags: -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck: --check-prefix=BYZANTIUM

interface Target {
    function read() external view returns (uint256);
}

contract Caller {
    // HOMESTEAD-LABEL: fn @read
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // HOMESTEAD-NOT: staticcall
    // HOMESTEAD-NOT: returndatasize
    // HOMESTEAD: revert_returndata
    // BYZANTIUM-LABEL: fn @read
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: staticcall
    // BYZANTIUM: revert_returndata
    function read(Target target) external view returns (uint256) {
        return target.read();
    }

    // HOMESTEAD-LABEL: fn @readPointer
    // HOMESTEAD: extcodesize
    // HOMESTEAD: call
    // HOMESTEAD-NOT: staticcall
    // HOMESTEAD-NOT: returndatasize
    // HOMESTEAD: revert_returndata
    // BYZANTIUM-LABEL: fn @readPointer
    // BYZANTIUM-NOT: extcodesize
    // BYZANTIUM: staticcall
    // BYZANTIUM: revert_returndata
    // BYZANTIUM: returndatasize
    function readPointer(function() external view returns (uint256) target)
        external
        view
        returns (uint256)
    {
        return target();
    }
}
