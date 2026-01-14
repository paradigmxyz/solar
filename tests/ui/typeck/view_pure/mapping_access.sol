//@compile-flags: -Ztypeck
// Tests that accessing mappings has correct mutability.
// TODO: When view/pure checking is implemented:
// readPure: function declared as pure, but this expression reads from the environment or state
// writeView: function cannot be declared as view because this expression modifies the state

contract C {
    mapping(uint => uint) m;

    function read(uint k) view public returns (uint) {
        return m[k];
    }
    function readPure(uint k) pure public returns (uint) {
        return m[k];
    }
    function write(uint k, uint v) public {
        m[k] = v;
    }
    function writeView(uint k, uint v) view public {
        m[k] = v;
    }
}
