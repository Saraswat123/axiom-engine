fn main() {
    // Compile guest into RISC-V ELF, embed as COMPUTE_ELF + COMPUTE_ID
    // Only runs when risc0-build feature enabled
    #[cfg(feature = "risc0")]
    risc0_build::embed_methods();
}
