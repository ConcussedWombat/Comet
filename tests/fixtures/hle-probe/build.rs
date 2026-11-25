fn main() {
    println!("cargo:rustc-link-arg-cdylib=/DELAYLOAD:comet-probe-delayed.dll");
    println!("cargo:rustc-link-lib=delayimp");
}
