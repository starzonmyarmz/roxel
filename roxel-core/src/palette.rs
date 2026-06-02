#[derive(Clone)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 4]>,
    pub builtin: bool,
}

#[macro_export]
macro_rules! hex_palette {
    ($name:expr, $($r:literal $g:literal $b:literal),* $(,)?) => {
        Palette {
            name: String::from($name),
            colors: vec![$([$r, $g, $b, 255u8]),*],
            builtin: true,
        }
    };
}
