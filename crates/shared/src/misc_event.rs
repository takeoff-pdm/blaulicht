pub mod videowall {
    /// Descriptor for adjusting the videowall brightness, expects a raw DMX-style value (0-255).
    pub const SET_BRIGHTNESS: u8 = 60;
    /// Descriptor for adjusting rotation, expects a midi value (0-127) that will be mapped to degrees.
    pub const SET_ROTATION: u8 = 61;
    /// Descriptor for selecting the active video by index within the known list.
    pub const SET_VIDEO_INDEX: u8 = 62;
    /// Descriptor for adjusting playback speed, expects a midi value (0-127) mapped to device semantics.
    pub const SET_SPEED: u8 = 63;
    /// Descriptor for requesting the videowall plugin to refresh status from the backend.
    pub const REQUEST_STATUS_REFRESH: u8 = 64;
    /// Descriptor for adjusting fry/contrast level, expects a midi value (0-127) mapped to tens.
    pub const SET_FRY: u8 = 65;
}
