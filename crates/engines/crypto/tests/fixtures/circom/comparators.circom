pragma circom 2.0.0;

template IsZero() {
    signal input in;
    signal output out;
    signal inv;

    inv <-- in!=0 ? 1/in : 0;
    out <== -in*inv + 1;
    in*out === 0;
}

template LessThan(n) {
    signal input in[2];
    signal output out;

    component n2b = Num2Bits(n+1);
    n2b.in <== in[0] + (1<<n) - in[1];
    out <== 1 - n2b.out[n];
}
