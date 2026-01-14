type U01 is U01;     //~ ERROR: the underlying type of UDVTs must be an elementary value type
type U02 is string;  //~ ERROR: the underlying type of UDVTs must be an elementary value type
type U03 is bytes;   //~ ERROR: the underlying type of UDVTs must be an elementary value type
type U04 is uint[];  //~ ERROR: the underlying type of UDVTs must be an elementary value type
type U05 is U06;     //~ ERROR: the underlying type of UDVTs must be an elementary value type

type U06 is address;
type U07 is address payable;
type U08 is uint;
type U09 is uint256;
type U10 is int;
type U11 is int256;
