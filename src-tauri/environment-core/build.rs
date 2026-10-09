fn main() {
    let target = std::env::var("TARGET").expect("Cargo TARGET is unavailable");
    println!("cargo:rustc-env=IYW_ENVIRONMENT_TARGET_TRIPLE={target}");
}
