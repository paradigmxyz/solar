pragma solidity *.*.*;
pragma solidity *.*.0 ;
pragma solidity *.*.0;
pragma solidity *.*;
pragma solidity *.0 .*;
pragma solidity *.0 .0 ;
pragma solidity *.0 .0;
pragma solidity *.0 ;
pragma solidity *.0.*;
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
pragma solidity 0 .0.*;
pragma solidity 0 .0.0 ;
pragma solidity 0 .0.0;
pragma solidity 0 .0;
pragma solidity 0 ;
pragma solidity 0.*.*;
pragma solidity 0.*.0 ;
pragma solidity 0.*.0;
pragma solidity 0.*;
pragma solidity 0.0 .*;
pragma solidity 0.0 .0 ;
pragma solidity 0.0 .0;
pragma solidity 0.0 ;
pragma solidity 0.0.*;
pragma solidity 0.0.0 ;
pragma solidity 0.0.0;
pragma solidity 0.0;
pragma solidity 0;

pragma solidity ^0.5.16 =0.8.22 || >=0.8.21 <=2 ~1 0.6.2;
pragma solidity 0.4 - 1 || 0.3 - 0.5.16;
// TODO: Technically valid but this requires re-implementing the entire version parser using the
// by-char parser, not just the number one, which is not worth it.
pragma solidity xX*x***X*X;
//~^ ERROR: unexpected trailing characters
//~| ERROR: expected version number
//~| ERROR: unexpected trailing characters

pragma solidity ^4294967295;
pragma solidity ^4294967296;
//~^ ERROR: too large
pragma solidity ^0.4294967296;
//~^ ERROR: too large

pragma solidity 0.;
//~^ ERROR: expected version number
pragma solidity 0.0.;
//~^ ERROR: expected version number
pragma solidity .0;
//~^ ERROR: expected version number
//~| ERROR: unexpected trailing characters

pragma solidity 88_;
//~^ ERROR: unexpected trailing characters
pragma solidity 88_e3;
//~^ ERROR: unexpected trailing characters
pragma solidity 0e1.0;
//~^ ERROR: expected version number
//~| ERROR: unexpected trailing characters
//~| ERROR: unexpected trailing characters
pragma solidity 0.0e1;
//~^ ERROR: unexpected trailing characters

pragma solidity 0 - 1 0 - 2;
//~^ ERROR: ranges can only be combined using the || operator
