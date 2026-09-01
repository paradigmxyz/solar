//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: unicodeLine => "😃, 😭, and 😈"
//@ run-call: asciiLine => "a   b"
// ported-from: test/libsolidity/semanticTests/strings/unicode_string.sol

contract StringLineContinuation {
    function unicodeLine() external pure returns (string memory) {
        return unicode"😃, 😭,\
 and 😈";
    }

    function asciiLine() external pure returns (string memory) {
        return "a \
  b";
    }
}
