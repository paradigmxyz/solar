contract C {
    function fixedLength(uint256[2] storage array) internal pure returns (uint256) {
        return array.length;
    }

    function dynamicLength(uint256[] storage array) internal pure returns (uint256) {
        return array.length;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }
}
