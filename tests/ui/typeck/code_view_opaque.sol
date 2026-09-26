// A `CodeView` is made only by `Code.slice`, from a range it checked, and
// only `Code` unpacks one: wrapping a word, decoding a view, or unwrapping one
// elsewhere is an error.
import {Code, CodeView} from "solar:core/v1/Code.sol";

contract Test {
    struct Holder {
        CodeView section;
    }

    function made(uint256 word) public pure returns (CodeView) {
        return CodeView.wrap(word); //~ ERROR: a `CodeView` can only be made by `Code`
    }

    function unpacked(CodeView section) public pure returns (uint256) {
        return CodeView.unwrap(section); //~ ERROR: the contents of a `CodeView` belong to `Code`
    }

    function decoded(bytes memory data) public pure returns (uint256) {
        CodeView section = abi.decode(data, (CodeView)); //~ ERROR: a `CodeView` can only be made by `Code`
        return Code.length(section);
    }

    function decodedInside(bytes memory data) public pure returns (uint256) {
        Holder memory holder = abi.decode(data, (Holder)); //~ ERROR: a `CodeView` can only be made by `Code`
        return Code.length(holder.section);
    }

    // The module's own functions are how a view is made and read.
    function sliced(address target) public view returns (uint256, uint256) {
        CodeView section = Code.slice(target, 0, 0);
        return (Code.offset(section), Code.length(Code.slice(section, 0, 0)));
    }
}
