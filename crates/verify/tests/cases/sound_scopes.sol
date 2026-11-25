pragma solidity ^0.8.20;

interface IG {
    function g() external returns (uint8);
}

contract Scope1 {
    uint256 internal x;
    IG internal ig;

    // After the loop, `y` is the outer local (1); the body's `y` is out of scope.
    /// @custom:ensures result == 5
    function bad_break_shadow() external pure returns (uint256 result) {
        uint256 y = 1;
        while (true) {
            uint256 y = 5;
            y;
            break;
        }
        result = y;
    }

    /// @custom:ensures result == 1
    function ok_break_shadow() external pure returns (uint256 result) {
        uint256 y = 1;
        while (true) {
            uint256 y = 5;
            y;
            break;
        }
        result = y;
    }

    // `return;` inside a block that shadows the state variable `x` leaves storage `x` alone.
    /// @custom:ensures x == 5
    function bad_state_shadow_return() external {
        x = 0;
        {
            uint256 x = 5;
            x;
            return;
        }
    }

    /// @custom:ensures x == 0
    function ok_state_shadow_return() external {
        x = 0;
        {
            uint256 x = 5;
            x;
            return;
        }
    }

    // The try clause's `z` is scoped to its block: after the try, `z` is the outer `a`.
    /// @custom:ensures result < 256
    function bad_try_leak(uint256 a) external returns (uint256 result) {
        uint256 z = a;
        try ig.g() returns (uint8 z) {
            z;
        } catch {
            revert();
        }
        result = z;
    }

    /// @custom:ensures result == a
    function ok_try_leak(uint256 a) external returns (uint256 result) {
        uint256 z = a;
        try ig.g() returns (uint8 z) {
            z;
        } catch {
            revert();
        }
        result = z;
    }

    function getX() internal view returns (uint256) {
        return x;
    }

    // `getX` reads storage `x` (0), not the caller's local named `x`.
    /// @custom:ensures result == 7
    function bad_call_sees_local() external returns (uint256 result) {
        x = 0;
        {
            uint256 x = 7;
            x;
            result = getX();
        }
    }

    /// @custom:ensures result == 0
    function ok_call_sees_local() external returns (uint256 result) {
        x = 0;
        {
            uint256 x = 7;
            x;
            result = getX();
        }
    }
}
