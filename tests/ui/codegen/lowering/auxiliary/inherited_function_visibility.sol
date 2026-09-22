function authorized(address caller) pure returns (bool) {
    return caller == address(0xdeadbeef);
}

function externalHelper() pure returns (uint256) { return 3; }
function internalHelper() pure returns (uint256) { return 100; }
function publicHelper() pure returns (uint256) { return 200; }
