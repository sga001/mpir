# mpir

This version of mpir works with SealPIR-Rust (which is an older version of SealPIR based on SEAL 2.3.1). 
It is here for purposes of reproducing the results of our [paper](https://eprint.iacr.org/2017/1142.pdf).

What you need: 

(1) SealPIR-Rust (https://github.com/sga001/sealpir-rust).

(2) Rust nightly. We have tested with ``rustc 1.68.0-nightly (d6f99e535 2023-01-02)``.

# Building

- Compile SealPIR-Rust and test it to make sure it works. 

You can compile and test SealPIR-Rust with cargo: ``$ cargo test``
 
- Modify the Cargo.toml file in this repo (mpir) to specify the path of SealPIR-rust (right now it assumes it is located 
at ``../sealpir-rust/``).

- Compile mpir with cargo and test that it works: ``$ cargo test``.


# Reproducing results

Run ``cargo bench`` to reproduce the experiments in the paper.
