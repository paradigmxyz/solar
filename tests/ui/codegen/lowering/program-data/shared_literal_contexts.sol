//@ revisions: gas size
//@[gas] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[size] compile-flags: -Osize -Zdump=evm-ir-runtime
//@ filecheck:
//@ run-call: hash => 0x5859102116b997afcded9babbe0cbc20b6dab889dfd147a4dc7959adaa6fefde

// One literal that a contract returns, reverts with and emits is pooled once: each context stores
// different words around it, and `pack-data` copies the words they have in common.
// CHECK: literal_0: hex"
// CHECK-NOT: literal_1
contract SharedLiteralContexts {
    event Seen(bytes data);

    function returned() public pure returns (bytes memory) {
        return "fnwnujpgzpzqfqqhzaalsncezyhhwbnxyntobkrpwdulaexcdzaofrxkmpefvbphcyggvzvyysxcdrprmgkernjloqmkpmiacfwwpapcrtxqhstkrtdfvfulklytjhdfcfynlfvbvqvhnfxsalpqmvhhfrcoznyxpbjbivjbnhfmckueexxpbieuqxpaglzeltynpspjersieyxamvewaiskuhsnivmyzeeedmdfauozecdujejrkmphtfffmgwxrybtwqxybzamocmysfwhiwtkxbhmejxjqzrpsdrtbqtxkqjjwtqyrgctkglckvndcucuzyoqtavuwopqqmmchabvnbhiwsmxnozhmmkphuucbpfplwtgyouuytvxtwvihppmjtmqwafkgjmwecrfcqjtmxxodfwngvklusvgicakqievqghtwiwmfmomnotvivfvmtidggblzuwlacqsbmtowoharedjgolalfesznqbjgnmrhhszsozavsccnxyvgebpqtrtppvzvkeieljjuuazcvcfqdvglilbfbspivzumfbdhlringwgxqnckhgqbxltruuufawlppfozaivbgv";
    }

    function reverts(uint256 x) external pure {
        require(x > 1, "fnwnujpgzpzqfqqhzaalsncezyhhwbnxyntobkrpwdulaexcdzaofrxkmpefvbphcyggvzvyysxcdrprmgkernjloqmkpmiacfwwpapcrtxqhstkrtdfvfulklytjhdfcfynlfvbvqvhnfxsalpqmvhhfrcoznyxpbjbivjbnhfmckueexxpbieuqxpaglzeltynpspjersieyxamvewaiskuhsnivmyzeeedmdfauozecdujejrkmphtfffmgwxrybtwqxybzamocmysfwhiwtkxbhmejxjqzrpsdrtbqtxkqjjwtqyrgctkglckvndcucuzyoqtavuwopqqmmchabvnbhiwsmxnozhmmkphuucbpfplwtgyouuytvxtwvihppmjtmqwafkgjmwecrfcqjtmxxodfwngvklusvgicakqievqghtwiwmfmomnotvivfvmtidggblzuwlacqsbmtowoharedjgolalfesznqbjgnmrhhszsozavsccnxyvgebpqtrtppvzvkeieljjuuazcvcfqdvglilbfbspivzumfbdhlringwgxqnckhgqbxltruuufawlppfozaivbgv");
    }

    function emits() external {
        emit Seen("fnwnujpgzpzqfqqhzaalsncezyhhwbnxyntobkrpwdulaexcdzaofrxkmpefvbphcyggvzvyysxcdrprmgkernjloqmkpmiacfwwpapcrtxqhstkrtdfvfulklytjhdfcfynlfvbvqvhnfxsalpqmvhhfrcoznyxpbjbivjbnhfmckueexxpbieuqxpaglzeltynpspjersieyxamvewaiskuhsnivmyzeeedmdfauozecdujejrkmphtfffmgwxrybtwqxybzamocmysfwhiwtkxbhmejxjqzrpsdrtbqtxkqjjwtqyrgctkglckvndcucuzyoqtavuwopqqmmchabvnbhiwsmxnozhmmkphuucbpfplwtgyouuytvxtwvihppmjtmqwafkgjmwecrfcqjtmxxodfwngvklusvgicakqievqghtwiwmfmomnotvivfvmtidggblzuwlacqsbmtowoharedjgolalfesznqbjgnmrhhszsozavsccnxyvgebpqtrtppvzvkeieljjuuazcvcfqdvglilbfbspivzumfbdhlringwgxqnckhgqbxltruuufawlppfozaivbgv");
    }

    function hash() external pure returns (bytes32) {
        return keccak256(returned());
    }
}
