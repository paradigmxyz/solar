// ported-from: test/libsolidity/syntaxTests/constants/initialization/block_tx_msg_property.sol
bytes32 constant blockhGlobal = blockhash(1); //~ ERROR: initial value for constant variable has to be compile-time constant
bytes32 constant blobhGlobal = blobhash(1); //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant bfGlobal = block.basefee; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant blobbfGlobal = block.blobbasefee; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant chainIdGlobal = block.chainid; //~ ERROR: initial value for constant variable has to be compile-time constant
address constant coinbaseGlobal = block.coinbase; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant diffGlobal = block.difficulty; //~ WARN: since Paris, `block.difficulty` was replaced by `block.prevrandao`
//~^ ERROR: initial value for constant variable has to be compile-time constant
uint constant gaslimitGlobal = block.gaslimit; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant numberGlobal = block.number; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant prevrandaoGlobal = block.prevrandao; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant timestampGlobal = block.timestamp; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant gGlobal = gasleft(); //~ ERROR: initial value for constant variable has to be compile-time constant
bytes constant dataGlobal = msg.data; //~ ERROR: initial value for constant variable has to be compile-time constant
address constant senderGlobal = msg.sender; //~ ERROR: initial value for constant variable has to be compile-time constant
bytes4 constant sigGlobal = msg.sig; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant valueGlobal = msg.value; //~ ERROR: initial value for constant variable has to be compile-time constant
uint constant gaspriceGlobal = tx.gasprice; //~ ERROR: initial value for constant variable has to be compile-time constant
address constant originGlobal = tx.origin; //~ ERROR: initial value for constant variable has to be compile-time constant

contract A {
    bytes32 constant blockh = blockhash(1); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes32 constant blobh = blobhash(1); //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant bf = block.basefee; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant blobbf = block.blobbasefee; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant chainId = block.chainid; //~ ERROR: initial value for constant variable has to be compile-time constant
    address constant coinbase = block.coinbase; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant diff = block.difficulty; //~ WARN: since Paris, `block.difficulty` was replaced by `block.prevrandao`
    //~^ ERROR: initial value for constant variable has to be compile-time constant
    uint constant gaslimit = block.gaslimit; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant number = block.number; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant prevrandao = block.prevrandao; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant timestamp = block.timestamp; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant g = gasleft(); //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes constant data = msg.data; //~ ERROR: initial value for constant variable has to be compile-time constant
    address constant sender = msg.sender; //~ ERROR: initial value for constant variable has to be compile-time constant
    bytes4 constant sig = msg.sig; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant value = msg.value; //~ ERROR: initial value for constant variable has to be compile-time constant
    uint constant gasprice = tx.gasprice; //~ ERROR: initial value for constant variable has to be compile-time constant
    address constant origin = tx.origin; //~ ERROR: initial value for constant variable has to be compile-time constant
}
