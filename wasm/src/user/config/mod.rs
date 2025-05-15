mod fixture;

use fixture::Fixture;
use serde::Deserialize;

pub type StrobeGroup = Vec<Fixture>;
pub type MoodGroup = Vec<Fixture>;

#[derive(Deserialize)]
pub struct Config {
    strobe_groups: Vec<StrobeGroup>,
    mood_groups: Vec<MoodGroup>,
    dimmer_groups: Vec<MoodGroup>,
}
