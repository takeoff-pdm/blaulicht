//
// Macros.
//

#[macro_export]
macro_rules! if_beat_strobe {
    ($input:expr, $state:expr, $body: expr, $not_body: expr) => {
        let mut time_between_beats_millis = $input.time_between_beats_millis as f32;

        // Apply modifier on this value.

        // if !$state.animation.strobe.controls.on_beat {
        //     // time_between_beats_millis = 333.333; // 180bpm.
        // }

        time_between_beats_millis *= $state.animation.strobe.controls.speed_multiplier;

        if (time_between_beats_millis > 0.0
            && $input.time - $state.last_beat_time >= time_between_beats_millis as u32)
        {
            // Check that there is a beat.
            if ($input.bass_avg >= 80)
                || ($input.bass_avg_short == 255)
                || ($input.bass >= 80 && $input.bass_avg <= 50)
                || ($input.bass_avg >= 70)
                || !$state.animation.strobe.controls.on_beat
            {
                $state.last_beat_time = $input.time;
                $body;
            } else {
                $not_body;
            }
        } else {
            $not_body;
        }
    };
}

#[macro_export]
macro_rules! if_beat_mood {
    ($input:expr, $state:expr, $body: expr, $not_body: expr) => {
        let mut time_between_beats_millis = $input.time_between_beats_millis as f32;

        // Apply modifier on this value.

        // if !$state.animation.strobe.controls.on_beat {
        //     // time_between_beats_millis = 333.333; // 180bpm.
        // }

        time_between_beats_millis *= $state.animation.mood.animation_speed_beat;

        if (time_between_beats_millis > 0.0
            && $input.time - $state.last_beat_time_mood >= time_between_beats_millis as u32)
        {
            // Check that there is a beat.
            if ($input.bass_avg >= 80)
                || ($input.bass_avg_short == 255)
                || ($input.bass >= 80 && $input.bass_avg <= 50)
                || ($input.bass_avg >= 70)
                || !$state.animation.strobe.controls.on_beat
            {
                $state.last_beat_time_mood = $input.time;
                $body;
            } else {
                $not_body;
            }
        } else {
            $not_body;
        }
    };
}
