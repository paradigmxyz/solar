//@ codegen-matrix: standard
//@ run-call: assign() => 2, 4, 5

contract StorageDynamicArrayFromFixed {
    enum Action {
        Zero,
        One,
        Two,
        Three,
        Add,
        Remove
    }

    Action[] actions;

    function assign() external returns (uint256 len, Action first, Action second) {
        actions = [Action.Add, Action.Remove];
        return (actions.length, actions[0], actions[1]);
    }
}
