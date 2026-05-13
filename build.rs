fn main() {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icons/roxel.ico");
        if let Err(e) = res.compile() {
            eprintln!("winres failed: {e}");
            std::process::exit(1);
        }
    }
}
