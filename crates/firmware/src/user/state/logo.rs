#[derive(Debug, PartialEq)]
pub enum LogoMode {
    Drop,
    Breakdown,
    Normal,
}

impl LogoMode {
    pub fn bytes(&self) -> &[u8] {
        match self {
            LogoMode::Drop => b"D1",
            LogoMode::Breakdown => b"D0",
            LogoMode::Normal => b"D2",
        }
    }
}