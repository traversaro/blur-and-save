fn main() -> std::io::Result<()> {
    println!("cargo:rerun-if-changed=assets/blur-and-save.ico");

    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/blur-and-save.ico")
        .set("ProductName", "Blur & Save")
        .set("FileDescription", "Blur & Save")
        .compile()?;

    Ok(())
}
