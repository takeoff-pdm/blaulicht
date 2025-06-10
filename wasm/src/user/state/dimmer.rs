#[derive(Debug, Clone, Copy)]
pub struct Dimmer {
    pub stage_real_brightness: u8,
    pub controls: DimmerControls,
}

#[derive(Debug, Clone, Copy)]
pub struct DimmerControls {
    pub brightness_stage: u8,
    pub brightness_other: u8,
}