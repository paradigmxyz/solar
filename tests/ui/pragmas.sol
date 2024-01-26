// Commented out are lexed as empty literals "0."
pragma solidity *.*.*;
pragma solidity *.*.0 ;
pragma solidity *.*.0;
pragma solidity *.*;
pragma solidity *.0 .*;
pragma solidity *.0 .0 ;
pragma solidity *.0 .0;
pragma solidity *.0 ;
// pragma solidity *.0.*;
pragma solidity *.0.0 ;
pragma solidity *.0.0;
pragma solidity *.0;
pragma solidity *;
pragma solidity 0 .*.*;
pragma solidity 0 .*.0 ;
pragma solidity 0 .*.0;
pragma solidity 0 .*;
pragma solidity 0 .0 .*;
pragma solidity 0 .0 .0 ;
pragma solidity 0 .0 .0;
pragma solidity 0 .0 ;
// pragma solidity 0 .0.*;
pragma solidity 0 .0.0 ;
pragma solidity 0 .0.0;
pragma solidity 0 .0;
pragma solidity 0 ;
// pragma solidity 0.*.*;
// pragma solidity 0.*.0 ;
// pragma solidity 0.*.0;
// pragma solidity 0.*;
pragma solidity 0.0 .*;
pragma solidity 0.0 .0 ;
pragma solidity 0.0 .0;
pragma solidity 0.0 ;
pragma solidity 0.0.*;
pragma solidity 0.0.0 ;
pragma solidity 0.0.0;
pragma solidity 0.0;
pragma solidity 0;

// pragma foo bar;
// ~^ ERROR unknown pragma
pragma abicoder v2;
// pragma solidity ^4294967295;
// ~^ ERROR too large
pragma solidity 0 - 1 0 - 2;
//~^ ERROR ranges can only be combined using the || operator
pragma abicoder "v2";
pragma solidity ^0.5.16 =0.8.22 || >=0.8.21 <=2 ~1 0.6.2;
pragma solidity 0.4 - 1 || 0.3 - 0.5.16;
