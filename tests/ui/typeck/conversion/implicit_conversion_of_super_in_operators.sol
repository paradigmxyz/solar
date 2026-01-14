contract C {
    function foo() public {
        super << this;
        super >> this;
        super ^ this;
        super | this;
        super & this;

        super * this;
        super / this;
        super % this;
        super - this;
        super + this;
        super ** this;

        super == this;
        super != this;
        super >= this;
        super <= this;
        super < this;
        super > this;

        super || this;
        super && this;

        super -= this;
        super += this;

        true ? super : this;
    }
}
