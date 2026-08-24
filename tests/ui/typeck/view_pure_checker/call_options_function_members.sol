contract C {
    function h() external payable {}

    function selectorWithoutOptions() external pure returns (bytes4) {
        return this.h.selector;
    }

    function selectorWithOptions() external pure returns (bytes4) {
        return this.h{gas: 42}.selector;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }
}
