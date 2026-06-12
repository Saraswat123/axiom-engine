fn main() {
    #[cfg(feature = "risc0")]
    {
        println!("cargo:rerun-if-changed=guest/src/main.rs");
        println!("cargo:rerun-if-changed=guest/Cargo.toml");
        risc0_build::embed_methods();
    }
}
