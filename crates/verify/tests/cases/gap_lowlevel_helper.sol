pragma solidity ^0.8.20;
// Bytes built by abi.encodeCall and handed to a helper that makes a low-level call with them:
// the call copies them out, so the helper does not write them, even when the helper is a
// library function named `call` like the builtin.
interface IERC20 {
    function transfer(address to, uint256 amount) external returns (bool);
}

library TokenCall {
    function call(address token, bytes memory data) internal returns (bool ok) {
        (ok, ) = token.call(data);
    }
}

library Clobber {
    function call(bytes memory data) internal pure {
        data[0] = 0x01;
    }
}

contract LowLevelHelper {
    uint256 internal x;

    function _exec(address t, bytes memory data) internal returns (bool ok) {
        (ok, ) = t.call(data);
    }

    /// @custom:ensures r == 7
    function ok_helper(address t, address to, uint256 a) external returns (uint256 r) {
        bytes memory data = abi.encodeCall(IERC20.transfer, (to, a));
        _exec(t, data);
        r = 7;
    }

    /// @custom:ensures r == 7
    function ok_library_named_call(address t, address to, uint256 a) external returns (uint256 r) {
        bytes memory data = abi.encodeCall(IERC20.transfer, (to, a));
        TokenCall.call(t, data);
        r = 7;
    }

    /// @custom:ensures r == 7
    function ok_loop(address t, address to, uint256 a, uint256 n) external returns (uint256 r) {
        bytes memory data = abi.encodeCall(IERC20.transfer, (to, a));
        for (uint256 i = 0; i < n; i++) {
            _exec(t, data);
        }
        r = 7;
    }

    // The call may fail.
    /// @custom:ensures r
    function bad_result(address t, address to, uint256 a) external returns (bool r) {
        bytes memory data = abi.encodeCall(IERC20.transfer, (to, a));
        r = TokenCall.call(t, data);
    }

    // The target may call back into setX.
    /// @custom:ensures x == old(x)
    function bad_reentry(address t, address to, uint256 a) external returns (bool r) {
        bytes memory data = abi.encodeCall(IERC20.transfer, (to, a));
        r = _exec(t, data);
    }

    function _clobber(bytes memory data) internal pure {
        Clobber.call(data);
    }

    // A library function named `call` that writes the bytes is not the builtin.
    /// @custom:ensures r == 0
    function bad_clobber(bytes memory data) external pure returns (uint8 r) {
        require(data.length > 0 && data[0] == 0x00);
        bytes memory alias_ = data;
        _clobber(data);
        r = uint8(alias_[0]);
    }

    function setX(uint256 v) external {
        x = v;
    }
}
