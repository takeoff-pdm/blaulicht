#[derive(Debug, Clone)]
pub enum WasmUiOp {
    Label(String),
    Separator,
    Button { label: String, id: u8 },
    Checkbox { label: String, id: u8, checked: bool },
    Slider { label: String, id: u8, min: u8, max: u8, value: u8 },
}
