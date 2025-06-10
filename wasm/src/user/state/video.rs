#[derive(Debug, Clone, Copy)]
pub struct Video {
    // pub last_speed_update: u32,
    pub speed: f32,

    pub speed_bpm_sync: bool,
    pub speed_bpm_sync_last_factor: f32,
    pub speed_sync_last_update: u32,

    pub brightness: u8,
    pub brightness_strobe_synced: bool,
    pub fry: u8,

    pub file: VideoFile,
}

#[derive(Debug, Clone, Copy)]
pub enum VideoFile {
    Cheese,
    Grr,
    Swim,
    Cyonic,
    Jacky,
    Loveletter,
    Platzhalter,
    Molly,
    Hydra,
}

impl VideoFile {
    pub fn path(&self) -> &'static str {
        match self {
            VideoFile::Cheese => "cheese.webm",
            VideoFile::Grr => "grr.webm",
            VideoFile::Swim => "swim.webm",
            VideoFile::Cyonic => "cyonic.webm",
            VideoFile::Jacky => "jacky.webm",
            VideoFile::Loveletter => "loveletter.webm",
            VideoFile::Platzhalter => "platzhalter.webm",
            VideoFile::Molly => "molly.webm",
            VideoFile::Hydra => "HYDRA",
        }
    }

    pub fn speed(&self) -> f32 {
        match self {
            VideoFile::Cheese => 1.1,
            VideoFile::Grr => 4.0,
            VideoFile::Swim => 1.21,
            VideoFile::Hydra => 1.0,
            _ => 1.0,
        }
    }

    pub fn base_bpm(&self) -> usize {
        match self {
            VideoFile::Cheese => 120,
            VideoFile::Grr => 120,
            VideoFile::Swim => 60,
            VideoFile::Hydra => 60,
            _ => 60,
        }
    }
}