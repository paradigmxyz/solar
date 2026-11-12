contract C {
    function f(address payable a, address x) public {
        a.transfer(1); //~ WARN: `transfer` is deprecated and scheduled for removal
        bool ok = a.send(1); //~ WARN: `send` is deprecated and scheduled for removal
        require(a.send(2)); //~ WARN: `send` is deprecated and scheduled for removal
        a.send(3);
        //~^ WARN: `send` is deprecated and scheduled for removal
        //~| WARN: failure condition of `send` ignored
        payable(x).send(4);
        //~^ WARN: `send` is deprecated and scheduled for removal
        //~| WARN: failure condition of `send` ignored
        payable(x).transfer(5); //~ WARN: `transfer` is deprecated and scheduled for removal
        (a.send(6));
        //~^ WARN: `send` is deprecated and scheduled for removal
        (a.send)(7);
        //~^ WARN: `send` is deprecated and scheduled for removal
        //~| WARN: failure condition of `send` ignored
        ok;
    }
}
