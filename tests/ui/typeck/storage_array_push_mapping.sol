// Mapping entries cannot be copied, so a storage array whose element type
// contains a (nested) mapping does not support `push(value)`: the copy would
// move only the ordinary fields and the appended element's mapping would keep
// whatever the slots it grew into already held. solc rejects this with
// TypeError 8871, the same code as the assignment it is a copy of.
contract C {
    struct WithMapping {
        uint256 a;
        mapping(uint256 => uint256) m;
    }

    struct Nested {
        uint256 a;
        WithMapping inner;
    }

    struct Plain {
        uint256 a;
    }

    // The mapping is only reachable through the struct's own recursion, so
    // finding it requires descending through the cycle.
    struct Recursive {
        uint256 a;
        mapping(uint256 => Recursive) recurse;
    }

    WithMapping[] direct;
    Nested[] nested;
    WithMapping[][] outer;
    Plain[] plain;
    Recursive[] recursive;

    WithMapping source;
    Nested nestedSource;
    Recursive recursiveSource;

    function push() external {
        direct.push(source); //~ ERROR: storage arrays with nested mappings do not support `push(<arg>)`
        nested.push(nestedSource); //~ ERROR: storage arrays with nested mappings do not support `push(<arg>)`
        outer.push(direct); //~ ERROR: storage arrays with nested mappings do not support `push(<arg>)`
        recursive.push(recursiveSource); //~ ERROR: storage arrays with nested mappings do not support `push(<arg>)`
    }

    // A plain element type still copies, and `push()` without an argument
    // appends a zeroed element even when it contains a mapping.
    function allowed() external {
        Plain memory p;
        plain.push(p);
        direct.push();
        nested.push();
        recursive.push();
        direct[0].a = 1;
    }
}
