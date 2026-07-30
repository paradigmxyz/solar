//@revisions: homestead byzantium
//@[homestead] compile-flags: -Zcodegen -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD
//@[byzantium] compile-flags: -Zcodegen -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck: --check-prefix=BYZANTIUM

interface Target {
    function read() external view returns (uint256);
}

contract Caller {
    // HOMESTEAD-LABEL: fn @read
    // HOMESTEAD: call
    // HOMESTEAD-NOT: staticcall
    // HOMESTEAD-NOT: returndatasize
    // HOMESTEAD: revert 0, 0
    // BYZANTIUM-LABEL: fn @read
    // BYZANTIUM: staticcall
    // BYZANTIUM: returndatasize
    // BYZANTIUM: returndatacopy
    function read(Target target) external view returns (uint256) {
        return target.read();
    }

    // HOMESTEAD-LABEL: fn @readPointer
    // HOMESTEAD: call
    // HOMESTEAD-NOT: staticcall
    // HOMESTEAD-NOT: returndatasize
    // HOMESTEAD: revert 0, 0
    // BYZANTIUM-LABEL: fn @readPointer
    // BYZANTIUM: staticcall
    // BYZANTIUM: returndatasize
    // BYZANTIUM: returndatacopy
    function readPointer(function() external view returns (uint256) target)
        external
        view
        returns (uint256)
    {
        return target();
    }
}
